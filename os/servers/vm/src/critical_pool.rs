use alloc::boxed::Box;
use alloc::vec::Vec;

pub(crate) struct CriticalPool<T> {
    pool: Vec<Box<T>>,
    min_reserved: usize,
}

impl<T: Default> CriticalPool<T> {
    pub(crate) fn new(capacity: usize, min_reserved: usize) -> Self {
        let mut pool = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            pool.push(Box::new(T::default()));
        }
        Self { pool, min_reserved }
    }

    pub(crate) fn take(&mut self) -> Option<Box<T>> {
        self.pool.pop()
    }

    pub(crate) fn restore(&mut self, obj: Box<T>) {
        self.pool.push(obj);
    }

    pub(crate) fn needs_refill(&self) -> bool {
        self.pool.len() < self.min_reserved
    }

    pub(crate) fn refill(&mut self, capacity: usize) {
        while self.pool.len() < capacity {
            self.pool.push(Box::new(T::default()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct TestObj {
        _value: u64,
    }

    #[test]
    fn test_critical_pool_take_restore() {
        let mut pool = CriticalPool::<TestObj>::new(4, 2);

        assert!(!pool.needs_refill());

        let _a = pool.take().unwrap();
        let b = pool.take().unwrap();
        let _c = pool.take().unwrap();

        assert!(pool.needs_refill());

        pool.restore(b);

        assert!(!pool.needs_refill());
    }

    #[test]
    fn test_critical_pool_exhaustion() {
        let mut pool = CriticalPool::<TestObj>::new(2, 1);

        let _a = pool.take().unwrap();
        let _b = pool.take().unwrap();

        assert!(pool.take().is_none());
    }

    #[test]
    fn test_critical_pool_refill() {
        let mut pool = CriticalPool::<TestObj>::new(4, 2);

        let _a = pool.take().unwrap();
        let _b = pool.take().unwrap();
        let _c = pool.take().unwrap();
        let _d = pool.take().unwrap();
        assert!(pool.take().is_none());

        pool.refill(4);
        assert!(pool.take().is_some());
    }
}
