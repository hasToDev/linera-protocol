// Copyright (c) Zefchain Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use linera_views::{
    dynamo_db::DynamoDbContext,
    hash::{HashView, Hasher, HashingContext},
    impl_view,
    memory::{Batch, MemoryContext, MemoryStoreMap},
    rocksdb::{KeyValueOperations, RocksdbContext, DB},
    test_utils::LocalStackTestContext,
    views::{
        CollectionOperations, CollectionView, Context, LogOperations, LogView, MapOperations,
        MapView, QueueOperations, QueueView, ReentrantCollectionView, RegisterOperations,
        RegisterView, ScopedView, View, ViewError,
    },
};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::Arc,
};
use tokio::sync::Mutex;

#[allow(clippy::type_complexity)]
pub struct StateView<C> {
    pub x1: ScopedView<0, RegisterView<C, u64>>,
    pub x2: ScopedView<1, RegisterView<C, u32>>,
    pub log: ScopedView<2, LogView<C, u32>>,
    pub map: ScopedView<3, MapView<C, String, usize>>,
    pub queue: ScopedView<4, QueueView<C, u64>>,
    pub collection: ScopedView<5, CollectionView<C, String, LogView<C, u32>>>,
    pub collection2:
        ScopedView<6, CollectionView<C, String, CollectionView<C, String, RegisterView<C, u32>>>>,
    pub collection3: ScopedView<7, CollectionView<C, String, QueueView<C, u64>>>,
    pub collection4: ScopedView<8, ReentrantCollectionView<C, String, QueueView<C, u64>>>,
}

// This also generates `trait StateViewContext: Context ... {}`
impl_view!(StateView { x1, x2, log, map, queue, collection, collection2, collection3, collection4 };
           RegisterOperations<u64>,
           RegisterOperations<u32>,
           LogOperations<u32>,
           MapOperations<String, usize>,
           QueueOperations<u64>,
           CollectionOperations<String>,
           CollectionOperations<String>,
           CollectionOperations<String>
);

#[async_trait]
pub trait StateStore {
    type Context: StateViewContext<Extra = usize>;

    async fn load(&mut self, id: usize) -> Result<StateView<Self::Context>, ViewError>;
}

#[derive(Default)]
pub struct MemoryTestStore {
    states: HashMap<usize, Arc<Mutex<MemoryStoreMap>>>,
}

#[async_trait]
impl StateStore for MemoryTestStore {
    type Context = MemoryContext<usize>;

    async fn load(&mut self, id: usize) -> Result<StateView<Self::Context>, ViewError> {
        let state = self
            .states
            .entry(id)
            .or_insert_with(|| Arc::new(Mutex::new(BTreeMap::new())));
        log::trace!("Acquiring lock on {:?}", id);
        let context = MemoryContext::new(state.clone().lock_owned().await, id);
        StateView::load(context).await
    }
}

pub struct RocksdbTestStore {
    db: Arc<DB>,
    accessed_chains: BTreeSet<usize>,
}

impl RocksdbTestStore {
    fn new(db: DB) -> Self {
        Self {
            db: Arc::new(db),
            accessed_chains: BTreeSet::new(),
        }
    }
}

#[async_trait]
impl StateStore for RocksdbTestStore {
    type Context = RocksdbContext<usize>;

    async fn load(&mut self, id: usize) -> Result<StateView<Self::Context>, ViewError> {
        self.accessed_chains.insert(id);
        log::trace!("Acquiring lock on {:?}", id);
        let context = RocksdbContext::new(self.db.clone(), bcs::to_bytes(&id)?, id);
        StateView::load(context).await
    }
}

pub struct DynamoDbTestStore {
    localstack: LocalStackTestContext,
    accessed_chains: BTreeSet<usize>,
}

impl DynamoDbTestStore {
    pub async fn new() -> Result<Self, anyhow::Error> {
        Ok(DynamoDbTestStore {
            localstack: LocalStackTestContext::new().await?,
            accessed_chains: BTreeSet::new(),
        })
    }
}

#[async_trait]
impl StateStore for DynamoDbTestStore {
    type Context = DynamoDbContext<usize>;

    async fn load(&mut self, id: usize) -> Result<StateView<Self::Context>, ViewError> {
        self.accessed_chains.insert(id);
        log::trace!("Acquiring lock on {:?}", id);
        let (context, _) = DynamoDbContext::from_config(
            self.localstack.dynamo_db_config(),
            "test_table".parse().expect("Invalid table name"),
            vec![0],
            id,
        )
        .await
        .expect("Failed to create DynamoDB context");
        StateView::load(context).await
    }
}

#[derive(Debug)]
pub struct TestConfig {
    with_flush: bool,
    with_map: bool,
    with_queue: bool,
    with_log: bool,
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            with_flush: true,
            with_map: true,
            with_queue: true,
            with_log: true,
        }
    }
}

impl TestConfig {
    fn samples() -> Vec<TestConfig> {
        vec![
            TestConfig {
                with_flush: false,
                with_map: false,
                with_queue: false,
                with_log: false,
            },
            TestConfig {
                with_flush: true,
                with_map: false,
                with_queue: false,
                with_log: false,
            },
            TestConfig {
                with_flush: false,
                with_map: true,
                with_queue: false,
                with_log: false,
            },
            TestConfig {
                with_flush: false,
                with_map: false,
                with_queue: true,
                with_log: false,
            },
            TestConfig {
                with_flush: false,
                with_map: false,
                with_queue: false,
                with_log: true,
            },
            TestConfig::default(),
        ]
    }
}

#[cfg(test)]
async fn test_store<S>(
    store: &mut S,
    config: &TestConfig,
) -> <<S::Context as HashingContext>::Hasher as Hasher>::Output
where
    S: StateStore,
    ViewError: From<<<S as StateStore>::Context as Context>::Error>,
{
    let default_hash = {
        let mut view = store.load(1).await.unwrap();
        view.hash().await.unwrap()
    };
    {
        let mut view = store.load(1).await.unwrap();
        assert_eq!(view.x1.extra(), &1);
        let hash = view.hash().await.unwrap();
        assert_eq!(hash, default_hash);
        assert_eq!(view.x1.get(), &0);
        view.x1.set(1);
        view.rollback();
        assert_eq!(view.hash().await.unwrap(), hash);
        view.x2.set(2);
        assert_ne!(view.hash().await.unwrap(), hash);
        if config.with_log {
            view.log.push(4);
        }
        if config.with_queue {
            view.queue.push_back(8);
            assert_eq!(view.queue.front().await.unwrap(), Some(8));
            view.queue.push_back(7);
            view.queue.delete_front();
        }
        if config.with_map {
            view.map.insert("Hello".to_string(), 5);
            assert_eq!(view.map.indices().await.unwrap(), vec!["Hello".to_string()]);
            let mut count = 0;
            view.map
                .for_each_index(|_index: String| count += 1)
                .await
                .unwrap();
            assert_eq!(count, 1);
        }
        assert_eq!(view.x1.get(), &0);
        assert_eq!(view.x2.get(), &2);
        if config.with_log {
            assert_eq!(view.log.read(0..10).await.unwrap(), vec![4]);
        }
        if config.with_queue {
            assert_eq!(view.queue.read_front(10).await.unwrap(), vec![7]);
        }
        if config.with_map {
            assert_eq!(view.map.get(&"Hello".to_string()).await.unwrap(), Some(5));
        }
        {
            let subview = view
                .collection
                .load_entry("hola".to_string())
                .await
                .unwrap();
            subview.push(17);
            subview.push(18);
        }
        assert_eq!(
            view.collection.indices().await.unwrap(),
            vec!["hola".to_string()]
        );
        let mut count = 0;
        view.collection
            .for_each_index(|_index: String| count += 1)
            .await
            .unwrap();
        assert_eq!(count, 1);
        {
            let subview = view
                .collection
                .load_entry("hola".to_string())
                .await
                .unwrap();
            assert_eq!(subview.read(0..10).await.unwrap(), vec![17, 18]);
        }
    };
    let staged_hash = {
        let mut view = store.load(1).await.unwrap();
        assert_eq!(view.hash().await.unwrap(), default_hash);
        assert_eq!(view.x1.get(), &0);
        assert_eq!(view.x2.get(), &0);
        if config.with_log {
            assert_eq!(view.log.read(0..10).await.unwrap(), vec![]);
        }
        if config.with_queue {
            assert_eq!(view.queue.read_front(10).await.unwrap(), vec![]);
        }
        if config.with_map {
            assert_eq!(view.map.get(&"Hello".to_string()).await.unwrap(), None);
        }
        {
            let subview = view
                .collection
                .load_entry("hola".to_string())
                .await
                .unwrap();
            assert_eq!(subview.read(0..10).await.unwrap(), vec![]);
        }
        {
            let subview = view
                .collection2
                .load_entry("ciao".to_string())
                .await
                .unwrap();
            let subsubview = subview.load_entry("!".to_string()).await.unwrap();
            subsubview.set(3);
            assert_eq!(subsubview.get(), &3);
        }
        view.x1.set(1);
        if config.with_log {
            view.log.push(4);
        }
        if config.with_queue {
            view.queue.push_back(7);
        }
        if config.with_map {
            view.map.insert("Hello".to_string(), 5);
            view.map.insert("Hi".to_string(), 2);
            view.map.remove("Hi".to_string());
        }
        {
            let subview = view
                .collection
                .load_entry("hola".to_string())
                .await
                .unwrap();
            subview.push(17);
            subview.push(18);
        }
        if config.with_flush {
            view.save().await.unwrap();
        }
        let hash = view.hash().await.unwrap();
        view.save().await.unwrap();
        hash
    };
    {
        let mut view = store.load(1).await.unwrap();
        let stored_hash = view.hash().await.unwrap();
        assert_eq!(staged_hash, stored_hash);
        assert_eq!(view.x1.get(), &1);
        assert_eq!(view.x2.get(), &0);
        if config.with_log {
            assert_eq!(view.log.read(0..10).await.unwrap(), vec![4]);
        }
        if config.with_queue {
            view.queue.push_back(8);
            assert_eq!(view.queue.read_front(10).await.unwrap(), vec![7, 8]);
            assert_eq!(view.queue.read_front(1).await.unwrap(), vec![7]);
            assert_eq!(view.queue.read_back(10).await.unwrap(), vec![7, 8]);
            assert_eq!(view.queue.read_back(1).await.unwrap(), vec![8]);
            assert_eq!(view.queue.front().await.unwrap(), Some(7));
            assert_eq!(view.queue.back().await.unwrap(), Some(8));
            assert_eq!(view.queue.count(), 2);
            view.queue.delete_front();
            assert_eq!(view.queue.front().await.unwrap(), Some(8));
            view.queue.delete_front();
            assert_eq!(view.queue.front().await.unwrap(), None);
            assert_eq!(view.queue.count(), 0);
            view.queue.push_back(13);
        }
        if config.with_map {
            assert_eq!(view.map.get(&"Hello".to_string()).await.unwrap(), Some(5));
            assert_eq!(view.map.get(&"Hi".to_string()).await.unwrap(), None);
        }
        {
            let subview = view
                .collection
                .load_entry("hola".to_string())
                .await
                .unwrap();
            assert_eq!(subview.read(0..10).await.unwrap(), vec![17, 18]);
        }
        if config.with_flush {
            view.save().await.unwrap();
        }
        {
            let subview = view
                .collection2
                .load_entry("ciao".to_string())
                .await
                .unwrap();
            let subsubview = subview.load_entry("!".to_string()).await.unwrap();
            assert_eq!(subsubview.get(), &3);
        }
        assert_eq!(
            view.collection.indices().await.unwrap(),
            vec!["hola".to_string()]
        );
        view.collection.remove_entry("hola".to_string());
        assert_ne!(view.hash().await.unwrap(), stored_hash);
        view.save().await.unwrap();
    }
    {
        let mut view = store.load(1).await.unwrap();
        {
            let mut subview = view
                .collection4
                .try_load_entry("hola".to_string())
                .await
                .unwrap();
            assert_eq!(subview.read_front(10).await.unwrap(), vec![]);
            assert!(view
                .collection4
                .try_load_entry("hola".to_string())
                .await
                .is_err());
            if config.with_queue {
                subview.push_back(13);
                assert_eq!(subview.front().await.unwrap(), Some(13));
                subview.delete_front();
                assert_eq!(subview.front().await.unwrap(), None);
                assert_eq!(subview.count(), 0);
            }
        }
    }
    {
        let mut view = store.load(1).await.unwrap();
        {
            let subview = view
                .collection
                .load_entry("hola".to_string())
                .await
                .unwrap();
            assert_eq!(subview.read(0..10).await.unwrap(), vec![]);
        }
        if config.with_queue {
            assert_eq!(view.queue.front().await.unwrap(), Some(13));
            view.queue.delete_front();
            assert_eq!(view.queue.front().await.unwrap(), None);
            assert_eq!(view.queue.count(), 0);
        }
        view.write_delete().await.unwrap();
    }
    staged_hash
}

#[cfg(test)]
async fn test_views_in_memory_param(config: &TestConfig) {
    log::warn!("Testing config {:?} with memory", config);
    let mut store = MemoryTestStore::default();
    test_store(&mut store, config).await;
    assert_eq!(store.states.len(), 1);
    let entry = store.states.get(&1).unwrap().clone();
    assert!(entry.lock().await.is_empty());
}

#[tokio::test]
async fn test_views_in_memory() {
    for config in TestConfig::samples() {
        test_views_in_memory_param(&config).await
    }
}

#[cfg(test)]
async fn test_views_in_rocksdb_param(config: &TestConfig) {
    log::warn!("Testing config {:?} with rocksdb", config);
    let dir = tempfile::TempDir::new().unwrap();
    let mut options = rocksdb::Options::default();
    options.create_if_missing(true);

    let db = DB::open(&options, &dir).unwrap();
    let mut store = RocksdbTestStore::new(db);
    let hash = test_store(&mut store, config).await;
    assert_eq!(store.accessed_chains.len(), 1);
    assert_eq!(store.db.count_keys().await.unwrap(), 0);

    let mut store = MemoryTestStore::default();
    let hash2 = test_store(&mut store, config).await;
    assert_eq!(hash, hash2);
}

#[tokio::test]
async fn test_views_in_rocksdb() {
    for config in TestConfig::samples() {
        test_views_in_rocksdb_param(&config).await
    }
}

#[tokio::test]
#[ignore]
async fn test_views_in_dynamo_db() -> Result<(), anyhow::Error> {
    let mut store = DynamoDbTestStore::new().await?;
    let config = TestConfig::default();
    let hash = test_store(&mut store, &config).await;
    assert_eq!(store.accessed_chains.len(), 1);

    let mut store = MemoryTestStore::default();
    let hash2 = test_store(&mut store, &config).await;
    assert_eq!(hash, hash2);

    Ok(())
}

#[cfg(test)]
async fn test_store_rollback_kernel<S>(store: &mut S)
where
    S: StateStore,
    ViewError: From<<<S as StateStore>::Context as Context>::Error>,
{
    {
        let mut view = store.load(1).await.unwrap();
        view.queue.push_back(8);
        view.map.insert("Hello".to_string(), 5);
        let subview = view
            .collection
            .load_entry("hola".to_string())
            .await
            .unwrap();
        subview.push(17);
        view.save().await.unwrap();
    }
    {
        let mut view = store.load(1).await.unwrap();
        view.queue.push_back(7);
        view.map.insert("Hello".to_string(), 4);
        let subview = view
            .collection
            .load_entry("DobryDen".to_string())
            .await
            .unwrap();
        subview.push(16);
        view.rollback();
        view.save().await.unwrap();
    }
    {
        let mut view = store.load(1).await.unwrap();
        view.queue.clear();
        view.map.clear();
        view.collection.clear();
        view.rollback();
        view.save().await.unwrap();
    }
    {
        let mut view = store.load(1).await.unwrap();
        assert_eq!(view.queue.front().await.unwrap(), Some(8));
        assert_eq!(view.map.get(&"Hello".to_string()).await.unwrap(), Some(5));
        assert_eq!(
            view.collection.indices().await.unwrap(),
            vec!["hola".to_string()]
        );
    };
}

#[tokio::test]
async fn test_store_rollback() {
    let mut store = MemoryTestStore::default();
    test_store_rollback_kernel(&mut store).await;

    let dir = tempfile::TempDir::new().unwrap();
    let mut options = rocksdb::Options::default();
    options.create_if_missing(true);

    let db = DB::open(&options, &dir).unwrap();
    let mut store = RocksdbTestStore::new(db);
    test_store_rollback_kernel(&mut store).await;
}

#[tokio::test]
async fn test_collection_removal() -> anyhow::Result<()> {
    type EntryType = RegisterView<MemoryContext<()>, u8>;
    type CollectionViewType = CollectionView<MemoryContext<()>, u8, EntryType>;

    let state = Arc::new(Mutex::new(BTreeMap::new()));
    let context = MemoryContext::new(state.lock_owned().await, ());

    // Write a dummy entry into the collection.
    let mut collection = CollectionViewType::load(context.clone()).await?;
    let entry = collection.load_entry(1).await?;
    entry.set(1);
    let mut batch = Batch::default();
    collection.flush(&mut batch).await?;
    collection.context().write_batch(batch).await?;

    // Remove the entry from the collection.
    let mut collection = CollectionViewType::load(context.clone()).await?;
    collection.remove_entry(1);
    let mut batch = Batch::default();
    collection.flush(&mut batch).await?;
    collection.context().write_batch(batch).await?;

    // Check that the entry was removed.
    let mut collection = CollectionViewType::load(context.clone()).await?;
    assert!(!collection.indices().await?.contains(&1));

    Ok(())
}

async fn test_removal_api_first_second_condition(
    first_condition: bool,
    second_condition: bool,
) -> anyhow::Result<()> {
    type EntryType = RegisterView<MemoryContext<()>, u8>;
    type CollectionViewType = CollectionView<MemoryContext<()>, u8, EntryType>;

    let state = Arc::new(Mutex::new(BTreeMap::new()));
    let context = MemoryContext::new(state.lock_owned().await, ());

    // First add an entry `1` with value `100` and commit
    let mut collection: CollectionViewType = CollectionView::load(context.clone()).await?;
    let entry = collection.load_entry(1).await?;
    entry.set(100);
    let mut batch = Batch::default();
    collection.flush(&mut batch).await?;
    collection.context().write_batch(batch).await?;

    // Reload the collection view and remove the entry, but don't commit yet
    let mut collection: CollectionViewType = CollectionView::load(context.clone()).await?;
    collection.remove_entry(1);

    // Now, read the entry with a different value if a certain condition is true
    if first_condition {
        let entry = collection.load_entry(1).await?;
        entry.set(200);
    }

    // Finally, either commit or rollback based on some other condition
    if second_condition {
        // If rolling back, then the entry `1` still exists with value `100`.
        collection.rollback();
    }

    // We commit
    let mut batch = Batch::default();
    collection.flush(&mut batch).await?;
    collection.context().write_batch(batch).await?;

    let mut collection: CollectionViewType = CollectionView::load(context.clone()).await?;
    let expected_val = if second_condition {
        Some(100)
    } else if first_condition {
        Some(200)
    } else {
        None
    };
    match expected_val {
        Some(expected_val_i) => {
            let subview = collection.load_entry(1).await?;
            assert_eq!(subview.get(), &expected_val_i);
        }
        None => {
            assert!(!collection.indices().await?.contains(&1));
        }
    };
    Ok(())
}

#[tokio::test]
async fn test_removal_api() -> anyhow::Result<()> {
    for first_condition in [true, false] {
        for second_condition in [true, false] {
            test_removal_api_first_second_condition(first_condition, second_condition).await?;
        }
    }
    Ok(())
}
