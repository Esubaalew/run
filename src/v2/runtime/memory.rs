use crate::v2::{Error, Result};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct MemoryConfig {
    pub max_per_component: usize,
    pub pool_size: usize,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            max_per_component: 256 * 1024 * 1024,
            pool_size: 4 * 1024 * 1024 * 1024,
        }
    }
}

pub struct MemoryPool {
    config: MemoryConfig,
    allocated: Arc<Mutex<AllocatedState>>,
}

struct AllocatedState {
    total_bytes: usize,
    allocations: HashMap<u64, AllocationInfo>,
    next_id: u64,
}

#[derive(Debug)]
struct AllocationInfo {
    size: usize,
    component_id: Option<String>,
    created_at: std::time::Instant,
}

impl MemoryPool {
    pub fn new(config: MemoryConfig) -> Self {
        Self {
            config,
            allocated: Arc::new(Mutex::new(AllocatedState {
                total_bytes: 0,
                allocations: HashMap::new(),
                next_id: 1,
            })),
        }
    }

    pub fn allocate(&self) -> Result<super::instance::AllocatedMemory> {
        self.allocate_with_size(self.config.max_per_component)
    }

    pub fn allocate_with_size(&self, size: usize) -> Result<super::instance::AllocatedMemory> {
        let mut state = self.allocated.lock().unwrap();

        if size > self.config.max_per_component {
            return Err(Error::other(format!(
                "Requested size {} exceeds max per component {}",
                size, self.config.max_per_component
            )));
        }

        if state.total_bytes + size > self.config.pool_size {
            return Err(Error::other(format!(
                "Memory pool exhausted: {} + {} > {}",
                state.total_bytes, size, self.config.pool_size
            )));
        }

        let id = state.next_id;
        state.next_id += 1;
        state.total_bytes += size;
        state.allocations.insert(
            id,
            AllocationInfo {
                size,
                component_id: None,
                created_at: std::time::Instant::now(),
            },
        );

        Ok(super::instance::AllocatedMemory { id, size })
    }

    pub fn release(&self, memory: Option<super::instance::AllocatedMemory>) {
        if let Some(mem) = memory {
            let mut state = self.allocated.lock().unwrap();
            if let Some(info) = state.allocations.remove(&mem.id) {
                state.total_bytes = state.total_bytes.saturating_sub(info.size);
            }
        }
    }

    pub fn usage(&self) -> MemoryUsage {
        let state = self.allocated.lock().unwrap();
        MemoryUsage {
            allocated_bytes: state.total_bytes,
            pool_size: self.config.pool_size,
            allocation_count: state.allocations.len(),
        }
    }

    pub fn usage_percent(&self) -> f64 {
        let state = self.allocated.lock().unwrap();
        (state.total_bytes as f64 / self.config.pool_size as f64) * 100.0
    }

    pub fn can_allocate(&self, size: usize) -> bool {
        let state = self.allocated.lock().unwrap();
        size <= self.config.max_per_component && state.total_bytes + size <= self.config.pool_size
    }

    pub fn available(&self) -> usize {
        let state = self.allocated.lock().unwrap();
        let remaining = self.config.pool_size.saturating_sub(state.total_bytes);
        remaining.min(self.config.max_per_component)
    }

    pub fn associate(&self, allocation_id: u64, component_id: &str) {
        let mut state = self.allocated.lock().unwrap();
        if let Some(info) = state.allocations.get_mut(&allocation_id) {
            info.component_id = Some(component_id.to_string());
        }
    }

    pub fn get_component_allocations(&self, component_id: &str) -> Vec<(u64, usize)> {
        let state = self.allocated.lock().unwrap();
        state
            .allocations
            .iter()
            .filter(|(_, info)| info.component_id.as_deref() == Some(component_id))
            .map(|(&id, info)| (id, info.size))
            .collect()
    }

    pub fn release_component(&self, component_id: &str) {
        let mut state = self.allocated.lock().unwrap();
        let to_remove: Vec<u64> = state
            .allocations
            .iter()
            .filter(|(_, info)| info.component_id.as_deref() == Some(component_id))
            .map(|(&id, _)| id)
            .collect();

        for id in to_remove {
            if let Some(info) = state.allocations.remove(&id) {
                state.total_bytes = state.total_bytes.saturating_sub(info.size);
            }
        }
    }

    pub fn stats(&self) -> PoolStats {
        let state = self.allocated.lock().unwrap();

        let mut by_age = vec![0usize; 4];
        let now = std::time::Instant::now();

        for info in state.allocations.values() {
            let age = now.duration_since(info.created_at);
            let bucket = if age.as_secs() < 1 {
                0
            } else if age.as_secs() < 10 {
                1
            } else if age.as_secs() < 60 {
                2
            } else {
                3
            };
            by_age[bucket] += info.size;
        }

        PoolStats {
            total_allocated: state.total_bytes,
            pool_capacity: self.config.pool_size,
            allocation_count: state.allocations.len(),
            max_per_component: self.config.max_per_component,
            by_age_bytes: by_age,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MemoryUsage {
    pub allocated_bytes: usize,
    pub pool_size: usize,
    pub allocation_count: usize,
}

#[derive(Debug, Clone)]
pub struct PoolStats {
    pub total_allocated: usize,
    pub pool_capacity: usize,
    pub allocation_count: usize,
    pub max_per_component: usize,
    pub by_age_bytes: Vec<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_pool_basic() {
        let pool = MemoryPool::new(MemoryConfig {
            max_per_component: 1024,
            pool_size: 4096,
        });

        let alloc1 = pool.allocate_with_size(512).unwrap();
        assert_eq!(alloc1.size, 512);

        let usage = pool.usage();
        assert_eq!(usage.allocated_bytes, 512);
        assert_eq!(usage.allocation_count, 1);

        pool.release(Some(alloc1));

        let usage = pool.usage();
        assert_eq!(usage.allocated_bytes, 0);
    }

    #[test]
    fn test_memory_pool_limits() {
        let pool = MemoryPool::new(MemoryConfig {
            max_per_component: 100,
            pool_size: 200,
        });

        assert!(pool.allocate_with_size(150).is_err());

        let alloc1 = pool.allocate_with_size(100).unwrap();
        let alloc2 = pool.allocate_with_size(100).unwrap();

        assert!(pool.allocate_with_size(50).is_err());

        pool.release(Some(alloc1));
        pool.release(Some(alloc2));
    }
}
