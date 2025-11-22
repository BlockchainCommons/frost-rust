use std::{fs, path::Path};

use assert_cmd::Command;
use bc_components::PrivateKeyBase;
use bc_envelope::prelude::{Date, UR, UREncodable};
use bc_rand::make_fake_random_number_generator;
use bc_xid::{
    XIDDocument, XIDGeneratorOptions, XIDGenesisMarkOptions,
    XIDInceptionKeyOptions, XIDPrivateKeyOptions, XIDSigningOptions,
};
use frost::participants::OwnerRecord;
use predicates::prelude::*;
use provenance_mark::ProvenanceMarkResolution;
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

    let path = participants_file(temp.path());
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

    assert!(!participants_file(temp.path()).exists());
}

fn make_owner_xid_ur() -> String {
    let mut rng = make_fake_random_number_generator();
    let date = Date::from_ymd(2025, 12, 31);
    let private_key_base = PrivateKeyBase::new_using(&mut rng);

    let document = XIDDocument::new(
        XIDInceptionKeyOptions::PrivateKeyBase(private_key_base),
        XIDGenesisMarkOptions::Passphrase(
            "owner-password".to_string(),
            Some(ProvenanceMarkResolution::Quartile),
            Some(date),
            None,
        ),
    );
    let envelope = document
        .to_envelope(
            XIDPrivateKeyOptions::Include,
            XIDGeneratorOptions::default(),
            XIDSigningOptions::Inception,
        )
        .unwrap();
    XIDDocument::from_envelope(
        &envelope,
        None,
        bc_xid::XIDVerifySignature::Inception,
    )
    .expect("owner envelope parses");
    let ur_string = envelope.ur_string();

    let roundtrip = UR::from_ur_string(&ur_string).expect("roundtrip UR");
    assert_eq!(roundtrip.ur_type_str(), "envelope");
    OwnerRecord::from_signed_xid_ur(ur_string.clone()).unwrap();
    ur_string
}

fn run_frost(cwd: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::cargo_bin("frost").unwrap();
    cmd.current_dir(cwd);
    cmd.args(args);
    cmd
}

fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name);
    fs::read_to_string(path).unwrap().trim().to_owned()
}

fn participants_file(dir: &Path) -> std::path::PathBuf {
    dir.join("registry.json")
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
