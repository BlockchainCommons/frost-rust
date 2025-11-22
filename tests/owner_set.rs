mod common;

use std::fs;

use common::{fixture, registry_file, run_frost};
use frost::registry::OwnerRecord;
use predicates::prelude::*;
use serde_json::json;
use tempfile::TempDir;

#[test]
fn owner_set_with_participant_add_persists_both() {
    register_tags();

    let temp = TempDir::new().unwrap();
    let alice_participant = fixture("alice_signed_xid.txt");
    let owner_ur = make_owner_xid_ur();
    OwnerRecord::from_signed_xid_ur(owner_ur.clone()).unwrap();

    run_frost(temp.path(), &["owner", "set", &owner_ur])
        .assert()
        .success();

    run_frost(
        temp.path(),
        &["participant", "add", &alice_participant, "Alice"],
    )
    .assert()
    .success();

    let path = registry_file(temp.path());
    let content = fs::read_to_string(path).unwrap();
    let expected = json!({
        "owner": {
            "xid_document": owner_ur
        },
        "participants": {
            "ur:xid/hdcxwmkbiywnmkwdlprdjliowtdkprkpbszodnlychyklapdjzrohnwpwecefglolsbsfnpkjony": {
                "xid_document": "ur:xid/tpsplftpsplftpsotanshdhdcxwmkbiywnmkwdlprdjliowtdkprkpbszodnlychyklapdjzrohnwpwecefglolsbsoyaylstpsotansgylftanshfhdcxswkeatmoclaehlpezsprtkntgrparfihgosofmfnlrgltndysabkwlckykimemottansgrhdcxtnhluevohylpdadednfmrsdkcfvovdsfaaadpecllftytbhgmylapkbarsfhdthsoycsfncsfgoycscstpsoihfpjziniaihoyaxtpsotansghhdfzkizesfchbgmylycxcesplsatmelfctwdplbeidjkmklehetntyidasgevachftiyotielsidkomoynskpkknpfuojobyrkbncektdsiateluetctyklrgrpshdhfadfzwkesroaa",
                "pet_name": "Alice"
            }
        }
    });

    assert_registry_matches(&content, expected);
}

#[test]
fn owner_set_requires_private_keys() {
    register_tags();

    let temp = TempDir::new().unwrap();
    let unsigned_owner = fixture("alice_signed_xid.txt"); // lacks private keys

    run_frost(temp.path(), &["owner", "set", &unsigned_owner])
        .assert()
        .failure()
        .stderr(predicate::str::contains("must include private keys"));

    assert!(!registry_file(temp.path()).exists());
}

fn make_owner_xid_ur() -> String {
    let ur_string = fixture("dan_private_xid.txt");
    let roundtrip =
        bc_envelope::prelude::UR::from_ur_string(&ur_string).unwrap();
    assert_eq!(roundtrip.ur_type_str(), "xid");
    OwnerRecord::from_signed_xid_ur(ur_string.clone()).unwrap();
    ur_string
}

fn assert_registry_matches(actual: &str, expected: serde_json::Value) {
    fn normalize(input: &str) -> serde_json::Value {
        serde_json::from_str(input).unwrap()
    }

    assert_eq!(normalize(actual), expected);
}

fn register_tags() {
    bc_components::register_tags();
    bc_envelope::register_tags();
    provenance_mark::register_tags();
}
