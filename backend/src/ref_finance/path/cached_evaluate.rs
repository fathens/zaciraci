#![allow(dead_code)]

use anyhow;
use num_traits::{One, Zero};
use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::Hash;
use std::ops::{Add, Div, Mul, Sub};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct InnerError(Arc<anyhow::Error>);

impl InnerError {
    pub fn new(error: anyhow::Error) -> Self {
        Self(Arc::new(error))
    }

    pub fn into_error(self) -> anyhow::Error {
        match Arc::try_unwrap(self.0) {
            Ok(error) => error,
            Err(arc) => anyhow::anyhow!("{:?}", arc),
        }
    }
}

// 型エイリアスを定義して複雑な型を分割
type CacheResult<A> = Result<Option<Arc<A>>, InnerError>;
type CacheMap<M, A> = HashMap<M, CacheResult<A>>;
type ThreadSafeCache<M, A> = Arc<Mutex<CacheMap<M, A>>>;

pub struct CachedEvaluate<A, M, C> {
    cache: ThreadSafeCache<M, A>,
    calc_res: C,
}

impl<A, M, C> CachedEvaluate<A, M, C>
where
    A: Send + Sync,
    C: Fn(M) -> CacheResult<A>,
    M: Send + Sync + Copy + Hash + Debug + Eq + Ord + Zero + One,
    M: Add<Output = M> + Sub<Output = M> + Mul<Output = M> + Div<Output = M>,
    M: From<u128>,
{
    pub fn new(calc_res: C) -> Self {
        Self {
            cache: Arc::new(Mutex::new(CacheMap::new())),
            calc_res,
        }
    }

    pub fn evaluate(&self, input: M) -> CacheResult<A> {
        if input == M::zero() {
            return Err(InnerError::new(anyhow::anyhow!(
                "Zero is an invalid input."
            )));
        }
        let mut cache = self.cache.lock().unwrap();
        if let Some(result) = cache.get(&input) {
            return result.clone();
        }

        let result = (self.calc_res)(input);
        cache.insert(input, result.clone());
        result
    }

    pub fn get_result(&self, input: M) -> CacheResult<A> {
        let cache = self.cache.lock().unwrap();
        cache.get(&input).cloned().unwrap_or(Ok(None))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cached_evaluate() {
        // テスト用の評価関数
        let eval_fn = |x: u128| -> CacheResult<u128> {
            if x > 10 {
                Ok(Some(Arc::new(x)))
            } else if x == 0 {
                Err(InnerError::new(anyhow::anyhow!("ゼロは無効な入力です")))
            } else {
                Ok(None)
            }
        };

        let cached_eval = CachedEvaluate::new(eval_fn);

        // evaluateメソッドのテスト
        let result = cached_eval.evaluate(15);
        assert!(result.is_ok());
        assert_eq!(*result.unwrap().unwrap(), 15);

        // キャッシュのテスト
        let cached_result = cached_eval.evaluate(15);
        assert!(cached_result.is_ok());
        assert_eq!(*cached_result.unwrap().unwrap(), 15);

        // get_resultメソッドのテスト
        let get_result = cached_eval.get_result(15);
        assert!(get_result.is_ok());
        assert_eq!(*get_result.unwrap().unwrap(), 15);

        // エラー処理のテスト（None）
        let none_result = cached_eval.evaluate(5);
        assert!(none_result.is_ok());
        assert!(none_result.unwrap().is_none());

        // エラー処理のテスト（Error）
        let error_result = cached_eval.evaluate(0);
        assert!(error_result.is_err());
        let error = error_result.unwrap_err().into_error();
        assert_eq!(error.to_string(), "Zero is an invalid input.");
    }
}
