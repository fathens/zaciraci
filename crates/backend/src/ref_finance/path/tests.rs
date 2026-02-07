use super::*;

struct TestCalc {
    sorted_points: Vec<(u128, u128)>,
    input_value: u128,
}

impl TestCalc {
    fn maker(points: &[(u128, u128)]) -> impl Fn(u128) -> Self {
        if points.len() < 2 {
            panic!("points must be more than 2");
        }
        let mut sorted_points = points.to_vec();
        sorted_points.sort_by_key(|(a, _)| *a);
        move |input_value| TestCalc {
            sorted_points: sorted_points.clone(),
            input_value,
        }
    }

    fn calc_gain(&self) -> u128 {
        let pos = self
            .sorted_points
            .binary_search_by_key(&self.input_value, |(a, _)| *a);
        match pos {
            Ok(pos) => self.sorted_points[pos].1,
            Err(pos) => {
                if 0 < pos && pos < self.sorted_points.len() {
                    let p0 = self.sorted_points[pos - 1];
                    let p1 = self.sorted_points[pos];
                    let (x0, y0) = (p0.0 as i128, p0.1 as i128);
                    let (x1, y1) = (p1.0 as i128, p1.1 as i128);
                    let x = self.input_value as i128;
                    let y = (x - x0) * (y1 - y0) / (x1 - x0) + y0;
                    y as u128
                } else {
                    0
                }
            }
        }
    }
}

#[test]
fn test_test_calc() {
    {
        let maker = TestCalc::maker(&[(1, 1), (2, 2), (3, 3)]);
        let calc1 = maker(1);
        let calc2 = maker(2);
        let calc3 = maker(3);
        assert_eq!(calc1.calc_gain(), 1);
        assert_eq!(calc2.calc_gain(), 2);
        assert_eq!(calc3.calc_gain(), 3);
    }
    {
        let maker = TestCalc::maker(&[(1, 1), (3, 3)]);
        let calc1 = maker(1);
        let calc2 = maker(2);
        let calc3 = maker(3);
        assert_eq!(calc1.calc_gain(), 1);
        assert_eq!(calc2.calc_gain(), 2);
        assert_eq!(calc3.calc_gain(), 3);
    }
    {
        let maker = TestCalc::maker(&[(1, 1), (2, 2)]);
        let calc1 = maker(1);
        let calc2 = maker(2);
        let calc3 = maker(3);
        assert_eq!(calc1.calc_gain(), 1);
        assert_eq!(calc2.calc_gain(), 2);
        assert_eq!(calc3.calc_gain(), 0);
    }
    {
        let maker = TestCalc::maker(&[(10, 20), (30, 50)]);
        let calc1 = maker(1);
        let calc9 = maker(9);
        let calc20 = maker(20);
        assert_eq!(calc1.calc_gain(), 0);
        assert_eq!(calc9.calc_gain(), 0);
        assert_eq!(calc20.calc_gain(), 35);
    }
    {
        let maker = TestCalc::maker(&[(20, 20), (40, 40), (50, 30), (70, 50)]);
        let calc10 = maker(10);
        let calc30 = maker(30);
        let calc45 = maker(45);
        let calc55 = maker(55);
        let calc60 = maker(60);
        assert_eq!(calc10.calc_gain(), 0);
        assert_eq!(calc30.calc_gain(), 30);
        assert_eq!(calc45.calc_gain(), 35);
        assert_eq!(calc55.calc_gain(), 35);
        assert_eq!(calc60.calc_gain(), 40);
    }
}

#[test]
fn test_search_best_path() {
    let result_pair = |a: Arc<TestCalc>| (a.input_value, a.calc_gain());

    {
        let maker = TestCalc::maker(&[(1, 1), (2, 2), (3, 3)]);
        let calc = |value| {
            let calc = maker(value);
            Ok(Some(Arc::new(calc)))
        };
        let get_gain = |a: Arc<TestCalc>| a.calc_gain();
        let result = search_best_path(1, 2, 3, calc, get_gain).unwrap();
        assert_eq!(result.map(result_pair), Some((3, 3)));
    }
    {
        let maker = TestCalc::maker(&[(1, 1), (3, 3)]);
        let calc = |value| {
            let calc = maker(value);
            Ok(Some(Arc::new(calc)))
        };
        let get_gain = |a: Arc<TestCalc>| a.calc_gain();
        let result = search_best_path(1, 2, 3, calc, get_gain).unwrap();
        assert_eq!(result.map(result_pair), Some((3, 3)));
    }
    {
        let maker = TestCalc::maker(&[(1, 1), (2, 2)]);
        let calc = |value| {
            let calc = maker(value);
            Ok(Some(Arc::new(calc)))
        };
        let get_gain = |a: Arc<TestCalc>| a.calc_gain();
        let result = search_best_path(1, 2, 3, calc, get_gain).unwrap();
        assert_eq!(result.map(result_pair), Some((2, 2)));
    }
    {
        let maker = TestCalc::maker(&[(10, 20), (30, 50)]);
        let calc = |value| {
            let calc = maker(value);
            Ok(Some(Arc::new(calc)))
        };
        let get_gain = |a: Arc<TestCalc>| a.calc_gain();
        let result = search_best_path(1, 2, 30, calc, get_gain).unwrap();
        assert_eq!(result.map(result_pair), Some((30, 50)));
    }
    {
        let maker = TestCalc::maker(&[(20, 20), (40, 40), (50, 30), (70, 50)]);
        let calc = |value| {
            let calc = maker(value);
            Ok(Some(Arc::new(calc)))
        };
        let get_gain = |a: Arc<TestCalc>| a.calc_gain();
        let result = search_best_path(1, 30, 100, calc, get_gain).unwrap();
        assert_eq!(result.map(result_pair), Some((70, 50)));
    }
    {
        let maker = TestCalc::maker(&[(20, 0), (70, 0)]);
        let calc = |value| {
            let calc = maker(value);
            Ok(Some(Arc::new(calc)))
        };
        let get_gain = |a: Arc<TestCalc>| a.calc_gain();
        let result = search_best_path(1, 30, 100, calc, get_gain).unwrap();
        assert_eq!(result.map(result_pair), None);
    }
    {
        let maker = TestCalc::maker(&[(1, 10), (100, 10)]);
        let calc = |value| {
            let calc = maker(value);
            Ok(Some(Arc::new(calc)))
        };
        let get_gain = |a: Arc<TestCalc>| a.calc_gain();
        let result = search_best_path(1, 30, 100, calc, get_gain).unwrap();
        assert_eq!(result.map(result_pair), Some((1, 10)));
    }
    {
        let maker = TestCalc::maker(&[(1, 10), (70, 20), (100, 10)]);
        let calc = |value| {
            let calc = maker(value);
            Ok(Some(Arc::new(calc)))
        };
        let get_gain = |a: Arc<TestCalc>| a.calc_gain();
        let result = search_best_path(1, 40, 100, calc, get_gain).unwrap();
        assert_eq!(result.map(result_pair), Some((70, 20)));
    }
    {
        let maker = TestCalc::maker(&[(30, 20), (50, 10), (70, 10)]);
        let calc = |value| {
            let calc = maker(value);
            Ok(Some(Arc::new(calc)))
        };
        let get_gain = |a: Arc<TestCalc>| a.calc_gain();
        let result = search_best_path(1, 40, 100, calc, get_gain).unwrap();
        assert_eq!(result.map(result_pair), Some((30, 20)));
    }
    {
        let maker = TestCalc::maker(&[(30, 20), (50, 20), (70, 10)]);
        let calc = |value| {
            let calc = maker(value);
            Ok(Some(Arc::new(calc)))
        };
        let get_gain = |a: Arc<TestCalc>| a.calc_gain();
        let result = search_best_path(1, 40, 100, calc, get_gain).unwrap();
        assert_eq!(result.map(result_pair), Some((30, 20)));
    }
    {
        let maker = TestCalc::maker(&[(30, 10), (50, 20), (70, 10)]);
        let calc = |value| {
            let calc = maker(value);
            Ok(Some(Arc::new(calc)))
        };
        let get_gain = |a: Arc<TestCalc>| a.calc_gain();
        let result = search_best_path(1, 40, 100, calc, get_gain).unwrap();
        assert_eq!(result.map(result_pair), Some((50, 20)));
    }
    {
        let maker = TestCalc::maker(&[(30, 10), (50, 20), (70, 20)]);
        let calc = |value| {
            let calc = maker(value);
            Ok(Some(Arc::new(calc)))
        };
        let get_gain = |a: Arc<TestCalc>| a.calc_gain();
        let result = search_best_path(1, 40, 100, calc, get_gain).unwrap();
        assert_eq!(result.map(result_pair), Some((51, 20)));
    }
    {
        let maker = TestCalc::maker(&[(30, 10), (50, 10), (70, 20)]);
        let calc = |value| {
            let calc = maker(value);
            Ok(Some(Arc::new(calc)))
        };
        let get_gain = |a: Arc<TestCalc>| a.calc_gain();
        let result = search_best_path(1, 40, 100, calc, get_gain).unwrap();
        assert_eq!(result.map(result_pair), Some((70, 20)));
    }
    {
        let maker = TestCalc::maker(&[(30, 20), (50, 20), (70, 20)]);
        let calc = |value| {
            let calc = maker(value);
            Ok(Some(Arc::new(calc)))
        };
        let get_gain = |a: Arc<TestCalc>| a.calc_gain();
        let result = search_best_path(1, 40, 100, calc, get_gain).unwrap();
        assert_eq!(result.map(result_pair), Some((30, 20)));
    }
}

#[test]
fn test_rate_averate() {
    assert_eq!(rate_average(1_u128, 1), 1);
    assert_eq!(rate_average(1_u128, 100), 10);
    assert_eq!(rate_average(10_u128, 1000), 100);
    assert_eq!(rate_average(10_u128, 100000), 1000);
}

mod test_static {
    use crate::Result;
    use async_once_cell::OnceCell;
    use std::ops::Deref;
    use std::sync::{LazyLock, Mutex};
    use tokio::time::sleep;

    static CACHED_STRING: OnceCell<String> = OnceCell::new();
    async fn get_cached_string() -> Result<&'static String> {
        CACHED_STRING.get_or_try_init(mk_string()).await
    }

    static LIST: LazyLock<Mutex<Vec<String>>> = LazyLock::new(|| Mutex::new(vec![]));

    fn push_log(s: &str) {
        let mut list = LIST.lock().unwrap();
        list.push(s.to_string());
    }

    async fn mk_string() -> Result<String> {
        push_log("start mk_string");
        sleep(tokio::time::Duration::from_secs(1)).await;
        push_log("end mk_string");
        Ok("test".to_string())
    }

    #[tokio::test]
    async fn test_once_cell() {
        push_log("start 0");
        let r1 = get_cached_string().await.unwrap();
        push_log("end 0");

        push_log("start 1");
        let r2 = get_cached_string().await.unwrap();
        push_log("end 1");

        assert_eq!(r1, r2);
        let guard = LIST.lock().unwrap();
        let list = guard.deref();
        assert_eq!(
            list,
            &[
                "start 0",
                "start mk_string",
                "end mk_string",
                "end 0",
                "start 1",
                "end 1",
            ]
        );
    }
}
