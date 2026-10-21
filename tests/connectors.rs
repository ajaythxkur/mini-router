use std::path::Path;

use mini_router::loader::load_connectors;

#[test]
fn all_connector_files_are_valid() {
    let connectors =
        load_connectors(Path::new("connectors")).expect("all connector files should load");

    for c in &connectors {
        jsonschema::validator_for(&c.input)
            .unwrap_or_else(|e| panic!("{}: invalid input schema {e}", c.id));

        let (provider, endpoint) =
            c.id.split_once('#')
                .unwrap_or_else(|| panic!("{}: id must look like provider#endpoint", c.id));
        assert!(
            !provider.is_empty() && !endpoint.is_empty(),
            "{}: empty provider or endpoint",
            c.id
        );
    }
    // every connector file is accounted for
    assert_eq!(connectors.len(), 12);
}
