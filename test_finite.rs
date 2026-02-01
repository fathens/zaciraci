fn main() {
    let values = vec![
        1.0,
        0.0,
        -1.0,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NAN,
    ];
    
    for v in values {
        println!("{:10} -> is_finite: {:5}, > 0.0: {:5}", 
                 format!("{:?}", v), v.is_finite(), v > 0.0);
    }
}