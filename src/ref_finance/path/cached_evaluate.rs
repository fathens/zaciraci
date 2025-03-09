use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use anyhow;

#[derive(Debug, Clone)]
struct InnerError(Arc<anyhow::Error>);

impl InnerError {
    fn unwrap(self) -> anyhow::Error {
        Arc::try_unwrap(self.0).unwrap()
    }
}

pub struct CachedEvaluate<A, M, C, G> {
    cache: Arc<Mutex<HashMap<M, Result<Option<Arc<A>>, InnerError>>>>,
    calc_res: C,
    get_gain: G,
}

impl<A, M, C, G> CachedEvaluate<A, M, C, G>
where
    A: Send + Sync,
    C: Fn(M) -> Result<Option<Arc<A>>, InnerError>,
    G: Fn(Arc<A>) -> u128,
    M: Send + Sync + Copy + Hash + Debug + Eq + Ord + Zero + One,
    M: Add<Output = M> + Sub<Output = M> + Mul<Output = M> + Div<Output = M>,
    M: From<u128>,
{
    pub fn new(calc_res: C, get_gain: G) -> Self {
        Self {
            cache: Arc::new(Mutex::new(HashMap::new())),
            calc_res,
            get_gain,
        }
    }

    pub fn evaluate(&self, input: M) -> Result<Option<Arc<A>>, InnerError> {
        let mut cache = self.cache.lock().unwrap();
        if let Some(result) = cache.get(&input) {
            return result.clone();
        }

        let result = (self.calc_res)(input);
        cache.insert(input, result.clone());
        result
    }

    pub fn get_result(&self, input: M) -> Result<Option<Arc<A>>, InnerError> {
        let cache = self.cache.lock().unwrap();
        cache.get(&input).cloned().flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use anyhow::Error;

    #[test]
    fn test_cached_evaluate() {
        // テスト用の評価関数
        let eval_fn = |x: u32| -> Result<Option<Arc<u32>>, InnerError> {
            if x > 10 {
                Ok(Some(Arc::new(x)))
            } else {
                Ok(None)
            }
        };

        // テスト用のゲイン関数
        let gain_fn = |result: Arc<u32>| *result;

        let mut cached_eval = CachedEvaluate::new(eval_fn, gain_fn);

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
        assert!(get_result.is_some());
        assert_eq!(*get_result.unwrap().unwrap(), 15);

        // エラー処理のテスト
        let error_result = cached_eval.evaluate(5);
        assert!(error_result.is_ok());
        assert!(error_result.unwrap().is_none());
    }
}
