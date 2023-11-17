use model_checked_orchestration::report::Reporter;
use stateright::Checker;
use stateright::Model;
use model_checked_orchestration::model::OrchestrationModelCfg;
use std::collections::BTreeMap;

use model_checked_orchestration::resources::Meta;

// Check that the annotations on resource `a` are all set on resource `b`.
pub fn annotations_subset<T, U>(a: &T, b: &U) -> bool
where
    T: Meta,
    U: Meta,
{
    subset(&a.metadata().annotations, &b.metadata().annotations)
}

fn subset(m1: &BTreeMap<String, String>, m2: &BTreeMap<String, String>) -> bool {
    m1.iter().all(|(k, v)| m2.get(k).map_or(false, |w| v == w))
}

pub fn run(model: OrchestrationModelCfg, fn_name: &str) {
    println!("Running test {:?}", fn_name);
    if let Ok(explore_test) = std::env::var("MCO_EXPLORE_TEST") {
        if fn_name.ends_with(&explore_test) {
            explore(model);
            return;
        }
    }
    check(model)
}

fn check(model: OrchestrationModelCfg) {
    println!("Checking model");
    let am = model.into_abstract_model();
    let mut reporter = Reporter::new(&am);
    am.checker()
        .spawn_bfs()
        .report(&mut reporter)
        .assert_properties()
}

fn explore(model: OrchestrationModelCfg) {
    let host = "127.0.0.1";
    let port = 8080;
    println!("Exploring model, served on http://{}:{}", host, port);
    let am = model.into_abstract_model();
    am.checker().serve((host, port));
}
