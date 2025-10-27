use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize)]
pub struct RcCurrency { pub code: Option<String> }

#[derive(Deserialize)]
pub struct RcCountry {
    pub name: String,
    pub capital: Option<String>,
    pub region: Option<String>,
    pub population: Option<i64>,
    pub flag: Option<String>,
    pub currencies: Option<Vec<RcCurrency>>,
}

#[derive(Deserialize)]
pub struct ErRates { pub rates: HashMap<String, f64> }
