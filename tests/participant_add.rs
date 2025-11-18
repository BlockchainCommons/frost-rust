use std::{fs, path::Path};

use assert_cmd::Command;
use indoc::indoc;
use predicates::prelude::*;
use tempfile::TempDir;

#[rustfmt::skip]
const ALICE_REGISTRY_JSON: &str = indoc! {r#"
{
    "participants": {
        "ur:xid/hdcxwmkbiywnmkwdlprdjliowtdkprkpbszodnlychyklapdjzrohnwpwecefglolsbsfnpkjony": {
            "xid_document": "ur:xid/tpsplftpsplftpsotanshdhdcxwmkbiywnmkwdlprdjliowtdkprkpbszodnlychyklapdjzrohnwpwecefglolsbsoyaylstpsotansgylftanshfhdcxswkeatmoclaehlpezsprtkntgrparfihgosofmfnlrgltndysabkwlckykimemottansgrhdcxtnhluevohylpdadednfmrsdkcfvovdsfaaadpecllftytbhgmylapkbarsfhdthsoycsfncsfgoycscstpsoihfpjziniaihoyaxtpsotansghhdfzkizesfchbgmylycxcesplsatmelfctwdplbeidjkmklehetntyidasgevachftiyotielsidkomoynskpkknpfuojobyrkbncektdsiateluetctyklrgrpshdhfadfzwkesroaa",
            "pet_name": "Alice"
        }
    }
}
"#};

#[rustfmt::skip]
const ALICE_AND_BOB_REGISTRY_JSON: &str = indoc! {r#"
{
    "participants": {
        "ur:xid/hdcxuysflgfsmwjseozmhplehywpwdcnfwmtvskkkbtieerpsfmtwegoiysaeeylfsecdsfxhljz": {
            "xid_document": "ur:xid/tpsplftpsplftpsotanshdhdcxuysflgfsmwjseozmhplehywpwdcnfwmtvskkkbtieerpsfmtwegoiysaeeylfsecoyaylstpsotansgylftanshfhdcxtoiniabgotbtltwpfgnbcxlybznngywkfsflbabyamadwmuefgtyjecxmteefxjntansgrhdcxbatpyafttpyabewkcmutihvesklrhytehydavdimwpahbalnnsrsnyfzpkcehpfhoycsfncsfgoycscstpsoiafwjlidoyaxtpsotansghhdfzhlimcmkgkkhdpmvsmtiowezcnemnyapaaxvostosrpluaslaylasmuzmsatsotwdchwlwmpsheclgeltynteyleohdwlhdticwdsahrtsrykseptflosbwtkrhlybwoydntkpmem",
            "pet_name": "Bob"
        },
        "ur:xid/hdcxwmkbiywnmkwdlprdjliowtdkprkpbszodnlychyklapdjzrohnwpwecefglolsbsfnpkjony": {
            "xid_document": "ur:xid/tpsplftpsplftpsotanshdhdcxwmkbiywnmkwdlprdjliowtdkprkpbszodnlychyklapdjzrohnwpwecefglolsbsoyaylstpsotansgylftanshfhdcxswkeatmoclaehlpezsprtkntgrparfihgosofmfnlrgltndysabkwlckykimemottansgrhdcxtnhluevohylpdadednfmrsdkcfvovdsfaaadpecllftytbhgmylapkbarsfhdthsoycsfncsfgoycscstpsoihfpjziniaihoyaxtpsotansghhdfzkizesfchbgmylycxcesplsatmelfctwdplbeidjkmklehetntyidasgevachftiyotielsidkomoynskpkknpfuojobyrkbncektdsiateluetctyklrgrpshdhfadfzwkesroaa",
            "pet_name": "Alice"
        }
    }
}
"#};

#[test]
fn participant_add_creates_registry_and_is_idempotent() {
    let temp = TempDir::new().unwrap();
    let alice = fixture("alice_signed_xid.txt");

    run_frost(temp.path(), &["participant", "add", &alice, "Alice"])
        .assert()
        .success();

    let path = participants_file(temp.path());
    let initial_state = fs::read_to_string(&path).unwrap();
    assert_registry_matches(&initial_state, ALICE_REGISTRY_JSON);

    run_frost(temp.path(), &["participant", "add", &alice, "Alice"])
        .assert()
        .success()
        .stdout(predicate::str::contains("already recorded"));

    let second_state = fs::read_to_string(&path).unwrap();
    assert_registry_matches(&second_state, ALICE_REGISTRY_JSON);
}

#[test]
fn participant_add_supports_custom_registry_filename_in_cwd() {
    let temp = TempDir::new().unwrap();
    let alice = fixture("alice_signed_xid.txt");
    let registry_name = "alice_registry.json";

    run_frost(
        temp.path(),
        &[
            "participant",
            "add",
            &alice,
            "Alice",
            "--registry",
            registry_name,
        ],
    )
    .assert()
    .success();

    let path = temp.path().join(registry_name);
    let content = fs::read_to_string(path).unwrap();
    assert_registry_matches(&content, ALICE_REGISTRY_JSON);
}

#[test]
fn participant_add_supports_directory_registry_path() {
    let temp = TempDir::new().unwrap();
    let alice = fixture("alice_signed_xid.txt");

    run_frost(
        temp.path(),
        &[
            "participant",
            "add",
            &alice,
            "Alice",
            "--registry",
            "registries/",
        ],
    )
    .assert()
    .success();

    let path = temp.path().join("registries").join("registry.json");
    let content = fs::read_to_string(path).unwrap();
    assert_registry_matches(&content, ALICE_REGISTRY_JSON);
}

#[test]
fn participant_add_supports_path_with_custom_filename() {
    let temp = TempDir::new().unwrap();
    let alice = fixture("alice_signed_xid.txt");
    let arg = "registries/alice_registry.json";

    run_frost(
        temp.path(),
        &["participant", "add", &alice, "Alice", "--registry", arg],
    )
    .assert()
    .success();

    let path = temp.path().join("registries").join("alice_registry.json");
    let content = fs::read_to_string(path).unwrap();
    assert_registry_matches(&content, ALICE_REGISTRY_JSON);
}

#[test]
fn participant_add_conflicting_pet_name_fails() {
    let temp = TempDir::new().unwrap();
    let alice = fixture("alice_signed_xid.txt");
    let bob = fixture("bob_signed_xid.txt");

    run_frost(temp.path(), &["participant", "add", &alice, "Alice"])
        .assert()
        .success();

    run_frost(temp.path(), &["participant", "add", &bob, "Alice"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already used"));

    let content = fs::read_to_string(participants_file(temp.path())).unwrap();
    assert_registry_matches(&content, ALICE_REGISTRY_JSON);
}

#[test]
fn participant_add_records_multiple_participants() {
    let temp = TempDir::new().unwrap();
    let alice = fixture("alice_signed_xid.txt");
    let bob = fixture("bob_signed_xid.txt");

    run_frost(temp.path(), &["participant", "add", &alice, "Alice"])
        .assert()
        .success();

    run_frost(temp.path(), &["participant", "add", &bob, "Bob"])
        .assert()
        .success();

    let content = fs::read_to_string(participants_file(temp.path())).unwrap();
    assert_registry_matches(&content, ALICE_AND_BOB_REGISTRY_JSON);
}

#[test]
fn participant_add_requires_signed_document() {
    let temp = TempDir::new().unwrap();
    let unsigned = fixture("bob_unsigned_xid.txt");

    run_frost(temp.path(), &["participant", "add", &unsigned, "Unsigned"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "XID document must be signed by its inception key",
        ));

    assert!(!participants_file(temp.path()).exists());
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

fn assert_registry_matches(actual: &str, expected: &str) {
    fn normalize(input: &str) -> String {
        let value: serde_json::Value = serde_json::from_str(input).unwrap();
        serde_json::to_string_pretty(&value).unwrap()
    }

    assert_eq!(normalize(actual), normalize(expected));
}
