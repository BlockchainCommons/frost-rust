use std::{
    collections::{BTreeMap, HashMap},
    fs,
};

mod common;
use indoc::indoc;
use serde_json::Value;
use tempfile::TempDir;
use common::{fixture, run_frost};

#[test]
fn introductions_create_four_registries() {
    register_tags();

    let fixtures = Users::load();

    let mut registries = BTreeMap::new();

    for user in fixtures.all_users() {
        let temp = TempDir::new().unwrap();
        let others = fixtures.others(user.name);

        run_frost(
            temp.path(),
            &["registry", "owner", "set", &user.private_doc],
        )
            .assert()
            .success();

        for other in &others {
            run_frost(
                temp.path(),
                &[
                    "registry",
                    "participant",
                    "add",
                    &other.signed_doc,
                    other.name,
                ],
            )
            .assert()
            .success();
        }

        let path = temp.path().join("registry.json");
        let content = fs::read_to_string(&path).unwrap();
        registries
            .insert(user.name, (content, expected_registry_text(user.name)));
    }

    for (name, (actual, expected)) in registries {
        let actual_pretty: Value = serde_json::from_str(&actual).unwrap();
        let actual_text = serde_json::to_string_pretty(&actual_pretty).unwrap();
        assert_actual_expected!(
            actual_text,
            expected,
            "{}'s registry mismatch",
            name
        );
    }
}

#[derive(Clone)]
struct User {
    name: &'static str,
    signed_doc: String,
    private_doc: String,
}

struct Users {
    alice: User,
    bob: User,
    carol: User,
    dan: User,
}

impl Users {
    fn load() -> Self {
        let fixtures = vec![
            ("alice", "Alice"),
            ("bob", "Bob"),
            ("carol", "Carol"),
            ("dan", "Dan"),
        ];

        let mut users: HashMap<String, User> = HashMap::new();

        for (key, name) in fixtures {
            let signed = fixture(&format!("{key}_signed_xid.txt"));
            let private = fixture(&format!("{key}_private_xid.txt"));
            users.insert(
                name.to_string(),
                User {
                    name,
                    signed_doc: signed.clone(),
                    private_doc: private,
                },
            );
        }

        Self {
            alice: users.remove("Alice").unwrap(),
            bob: users.remove("Bob").unwrap(),
            carol: users.remove("Carol").unwrap(),
            dan: users.remove("Dan").unwrap(),
        }
    }

    fn all_users(&self) -> [User; 4] {
        [
            self.alice.clone(),
            self.bob.clone(),
            self.carol.clone(),
            self.dan.clone(),
        ]
    }

    fn others(&self, name: &str) -> Vec<User> {
        self.all_users()
            .into_iter()
            .filter(|user| user.name != name)
            .collect()
    }
}

fn expected_registry_text(owner_name: &str) -> String {
    match owner_name {
        "Alice" => indoc! {r#"
            {
              "owner": {
                "xid_document": "ur:xid/tpsplftpsotanshdhdcxwmkbiywnmkwdlprdjliowtdkprkpbszodnlychyklapdjzrohnwpwecefglolsbsoyaylrtpsotansgylftanshfhdcxswkeatmoclaehlpezsprtkntgrparfihgosofmfnlrgltndysabkwlckykimemottansgrhdcxtnhluevohylpdadednfmrsdkcfvovdsfaaadpecllftytbhgmylapkbarsfhdthsoycsfncsfgoycscstpsoihfpjziniaihlfoycsfptpsotansgtlftansgohdcxgrftdienhfprgtbsjlgmtefzfmhpvtlyqzglcxrstaeegdstlncwwtfhwkdwkbehtansgehdcxctzmfyqzrkcpjpbslrmeiymovtktbkdllynbztspryolfhjpbzrdmeghwdehkekpoybstpsotansgmhdcxbagdkturtovtaxryolhgonbblygahdsbwejyhspfbgtbspssmwroghhknyhlzcyafzpkftwy"
              },
              "participants": {
                "ur:xid/hdcxnemtgmchkkuewflyoycydstepepecfoxwyeoiszourtayketgutkhnoednceeydkrttabnpl": {
                  "pet_name": "Carol",
                  "xid_document": "ur:xid/tpsplftpsplftpsotanshdhdcxnemtgmchkkuewflyoycydstepepecfoxwyeoiszourtayketgutkhnoednceeydkoyaylstpsotansgylftanshfhdcxkkplksiykkynfeisiedkserfbdollodikgprmsihaeuoveehytcsjzmkahwycxestansgrhdcxfgbbinheylcmhlsomhrsvddmisvehhgywttnwlwniojecerettfeuyjylnlybsbtoycsfncsfgoycscstpsoihfxhsjpjljzoyaxtpsotansghhdfzfwgycsiocwnsimpafwytieyktsaegunneyvawkidaojzsrmndmptrygucnlelocpeydtnddmndgugakgdsoxwmpfadgumyesvwssgmpeiymddnsbcmbakkguaddernimwmoxdyta"
                },
                "ur:xid/hdcxptfslorobgsgbyltdeoxbniysoktlffllkdtkovadicljpbahhenemhtbsmslarovtfgurnd": {
                  "pet_name": "Dan",
                  "xid_document": "ur:xid/tpsplftpsplftpsotanshdhdcxptfslorobgsgbyltdeoxbniysoktlffllkdtkovadicljpbahhenemhtbsmslarooyaylstpsotansgylftanshfhdcxsaspkbhlmukgpfwemofgiadppkpsihrphecafxgahnbgfljnptluhdwzfdvacswftansgrhdcxldgtiadwmnvslaaxgufysnlfneghfgurfsiaaysbhkcadmaavwlkfytdwdzmkteeoycsfncsfgoycscstpsoiafyhsjtoyaxtpsotansghhdfzoxahrnfmmnckdedaiarkgonbnytdneesjtftwsgrrlaootployfzhtattdhnjkfwguctjtjnurutlomuatsokkjybzflfenlmnfpcecainkksoztcxdtjnmyotgyesqdkgadjtdi"
                },
                "ur:xid/hdcxuysflgfsmwjseozmhplehywpwdcnfwmtvskkkbtieerpsfmtwegoiysaeeylfsecdsfxhljz": {
                  "pet_name": "Bob",
                  "xid_document": "ur:xid/tpsplftpsplftpsotanshdhdcxuysflgfsmwjseozmhplehywpwdcnfwmtvskkkbtieerpsfmtwegoiysaeeylfsecoyaylstpsotansgylftanshfhdcxtoiniabgotbtltwpfgnbcxlybznngywkfsflbabyamadwmuefgtyjecxmteefxjntansgrhdcxbatpyafttpyabewkcmutihvesklrhytehydavdimwpahbalnnsrsnyfzpkcehpfhoycsfncsfgoycscstpsoiafwjlidoyaxtpsotansghhdfzhlimcmkgkkhdpmvsmtiowezcnemnyapaaxvostosrpluaslaylasmuzmsatsotwdchwlwmpsheclgeltynteyleohdwlhdticwdsahrtsrykseptflosbwtkrhlybwoydntkpmem"
                }
              }
            }
        "#}
        .trim()
        .to_string(),
        "Bob" => indoc! {r#"
            {
              "owner": {
                "xid_document": "ur:xid/tpsplftpsotanshdhdcxuysflgfsmwjseozmhplehywpwdcnfwmtvskkkbtieerpsfmtwegoiysaeeylfsecoyaylrtpsotansgylftanshfhdcxtoiniabgotbtltwpfgnbcxlybznngywkfsflbabyamadwmuefgtyjecxmteefxjntansgrhdcxbatpyafttpyabewkcmutihvesklrhytehydavdimwpahbalnnsrsnyfzpkcehpfhoycsfncsfglfoycsfptpsotansgtlftansgohdcxmdfskpescejtfyjozttezsbsbwbtmochadnscpadcwnnfejlqzjomwpskieyrendtansgehdcxdwbalolkwlhfhfvewnbbtshdbbiswdreldadwleynbfrwdgsbgjlgthfhktdrykooybstpsotansgmhdcxbsadwtqzdkvefnpesfasgsuoiocavonbieimfhtpkkpslbkbkkoscyidlfnscygdoycscstpsoiafwjlidclptjzwz"
              },
              "participants": {
                "ur:xid/hdcxnemtgmchkkuewflyoycydstepepecfoxwyeoiszourtayketgutkhnoednceeydkrttabnpl": {
                  "pet_name": "Carol",
                  "xid_document": "ur:xid/tpsplftpsplftpsotanshdhdcxnemtgmchkkuewflyoycydstepepecfoxwyeoiszourtayketgutkhnoednceeydkoyaylstpsotansgylftanshfhdcxkkplksiykkynfeisiedkserfbdollodikgprmsihaeuoveehytcsjzmkahwycxestansgrhdcxfgbbinheylcmhlsomhrsvddmisvehhgywttnwlwniojecerettfeuyjylnlybsbtoycsfncsfgoycscstpsoihfxhsjpjljzoyaxtpsotansghhdfzfwgycsiocwnsimpafwytieyktsaegunneyvawkidaojzsrmndmptrygucnlelocpeydtnddmndgugakgdsoxwmpfadgumyesvwssgmpeiymddnsbcmbakkguaddernimwmoxdyta"
                },
                "ur:xid/hdcxptfslorobgsgbyltdeoxbniysoktlffllkdtkovadicljpbahhenemhtbsmslarovtfgurnd": {
                  "pet_name": "Dan",
                  "xid_document": "ur:xid/tpsplftpsplftpsotanshdhdcxptfslorobgsgbyltdeoxbniysoktlffllkdtkovadicljpbahhenemhtbsmslarooyaylstpsotansgylftanshfhdcxsaspkbhlmukgpfwemofgiadppkpsihrphecafxgahnbgfljnptluhdwzfdvacswftansgrhdcxldgtiadwmnvslaaxgufysnlfneghfgurfsiaaysbhkcadmaavwlkfytdwdzmkteeoycsfncsfgoycscstpsoiafyhsjtoyaxtpsotansghhdfzoxahrnfmmnckdedaiarkgonbnytdneesjtftwsgrrlaootployfzhtattdhnjkfwguctjtjnurutlomuatsokkjybzflfenlmnfpcecainkksoztcxdtjnmyotgyesqdkgadjtdi"
                },
                "ur:xid/hdcxwmkbiywnmkwdlprdjliowtdkprkpbszodnlychyklapdjzrohnwpwecefglolsbsfnpkjony": {
                  "pet_name": "Alice",
                  "xid_document": "ur:xid/tpsplftpsplftpsotanshdhdcxwmkbiywnmkwdlprdjliowtdkprkpbszodnlychyklapdjzrohnwpwecefglolsbsoyaylstpsotansgylftanshfhdcxswkeatmoclaehlpezsprtkntgrparfihgosofmfnlrgltndysabkwlckykimemottansgrhdcxtnhluevohylpdadednfmrsdkcfvovdsfaaadpecllftytbhgmylapkbarsfhdthsoycsfncsfgoycscstpsoihfpjziniaihoyaxtpsotansghhdfzkizesfchbgmylycxcesplsatmelfctwdplbeidjkmklehetntyidasgevachftiyotielsidkomoynskpkknpfuojobyrkbncektdsiateluetctyklrgrpshdhfadfzwkesroaa"
                }
              }
            }
        "#}
        .trim()
        .to_string(),
        "Carol" => indoc! {r#"
            {
              "owner": {
                "xid_document": "ur:xid/tpsplftpsotanshdhdcxnemtgmchkkuewflyoycydstepepecfoxwyeoiszourtayketgutkhnoednceeydkoyaylrtpsotansgylftanshfhdcxkkplksiykkynfeisiedkserfbdollodikgprmsihaeuoveehytcsjzmkahwycxestansgrhdcxfgbbinheylcmhlsomhrsvddmisvehhgywttnwlwniojecerettfeuyjylnlybsbtlfoycsfptpsotansgtlftansgohdcxhpnnvtrooespntdrcnylluvwimbbamfzswaatifrvspyqdjyjooltacaasdthedrtansgehdcxrprtadaacyhtsgfglfwefhjzcmlapdasndgwfgnlvtaeuecwaezsclrpwfgmiyfloybstpsotansgmhdcxpdrevoptstsgjsbzoybwmelgwnhdcscaesnblapacfuolsdysfykflihjolbzovwoycsfncsfgoycscstpsoihfxhsjpjljzgljeqzfl"
              },
              "participants": {
                "ur:xid/hdcxptfslorobgsgbyltdeoxbniysoktlffllkdtkovadicljpbahhenemhtbsmslarovtfgurnd": {
                  "pet_name": "Dan",
                  "xid_document": "ur:xid/tpsplftpsplftpsotanshdhdcxptfslorobgsgbyltdeoxbniysoktlffllkdtkovadicljpbahhenemhtbsmslarooyaylstpsotansgylftanshfhdcxsaspkbhlmukgpfwemofgiadppkpsihrphecafxgahnbgfljnptluhdwzfdvacswftansgrhdcxldgtiadwmnvslaaxgufysnlfneghfgurfsiaaysbhkcadmaavwlkfytdwdzmkteeoycsfncsfgoycscstpsoiafyhsjtoyaxtpsotansghhdfzoxahrnfmmnckdedaiarkgonbnytdneesjtftwsgrrlaootployfzhtattdhnjkfwguctjtjnurutlomuatsokkjybzflfenlmnfpcecainkksoztcxdtjnmyotgyesqdkgadjtdi"
                },
                "ur:xid/hdcxuysflgfsmwjseozmhplehywpwdcnfwmtvskkkbtieerpsfmtwegoiysaeeylfsecdsfxhljz": {
                  "pet_name": "Bob",
                  "xid_document": "ur:xid/tpsplftpsplftpsotanshdhdcxuysflgfsmwjseozmhplehywpwdcnfwmtvskkkbtieerpsfmtwegoiysaeeylfsecoyaylstpsotansgylftanshfhdcxtoiniabgotbtltwpfgnbcxlybznngywkfsflbabyamadwmuefgtyjecxmteefxjntansgrhdcxbatpyafttpyabewkcmutihvesklrhytehydavdimwpahbalnnsrsnyfzpkcehpfhoycsfncsfgoycscstpsoiafwjlidoyaxtpsotansghhdfzhlimcmkgkkhdpmvsmtiowezcnemnyapaaxvostosrpluaslaylasmuzmsatsotwdchwlwmpsheclgeltynteyleohdwlhdticwdsahrtsrykseptflosbwtkrhlybwoydntkpmem"
                },
                "ur:xid/hdcxwmkbiywnmkwdlprdjliowtdkprkpbszodnlychyklapdjzrohnwpwecefglolsbsfnpkjony": {
                  "pet_name": "Alice",
                  "xid_document": "ur:xid/tpsplftpsplftpsotanshdhdcxwmkbiywnmkwdlprdjliowtdkprkpbszodnlychyklapdjzrohnwpwecefglolsbsoyaylstpsotansgylftanshfhdcxswkeatmoclaehlpezsprtkntgrparfihgosofmfnlrgltndysabkwlckykimemottansgrhdcxtnhluevohylpdadednfmrsdkcfvovdsfaaadpecllftytbhgmylapkbarsfhdthsoycsfncsfgoycscstpsoihfpjziniaihoyaxtpsotansghhdfzkizesfchbgmylycxcesplsatmelfctwdplbeidjkmklehetntyidasgevachftiyotielsidkomoynskpkknpfuojobyrkbncektdsiateluetctyklrgrpshdhfadfzwkesroaa"
                }
              }
            }
        "#}
        .trim()
        .to_string(),
        "Dan" => indoc! {r#"
            {
              "owner": {
                "xid_document": "ur:xid/tpsplftpsotanshdhdcxptfslorobgsgbyltdeoxbniysoktlffllkdtkovadicljpbahhenemhtbsmslarooyaylrtpsotansgylftanshfhdcxsaspkbhlmukgpfwemofgiadppkpsihrphecafxgahnbgfljnptluhdwzfdvacswftansgrhdcxldgtiadwmnvslaaxgufysnlfneghfgurfsiaaysbhkcadmaavwlkfytdwdzmkteeoycsfncsfglfoycsfptpsotansgtlftansgohdcxltmtiegwiddrbnvebzbkfzmholaadyvdkobdrlfswtlefyfrrhjzjpjoneetskjytansgehdcxurdshedtbtueldbszetsayhntnsnsgetcapscalpztgmcpinfppytizsprmevtfloybstpsotansgmhdcxsgylchiyotiamhkeftdafdvochskhpknembttoecbglbmyutytwtoxrpswhsdpuyoycscstpsoiafyhsjtgewthkps"
              },
              "participants": {
                "ur:xid/hdcxnemtgmchkkuewflyoycydstepepecfoxwyeoiszourtayketgutkhnoednceeydkrttabnpl": {
                  "pet_name": "Carol",
                  "xid_document": "ur:xid/tpsplftpsplftpsotanshdhdcxnemtgmchkkuewflyoycydstepepecfoxwyeoiszourtayketgutkhnoednceeydkoyaylstpsotansgylftanshfhdcxkkplksiykkynfeisiedkserfbdollodikgprmsihaeuoveehytcsjzmkahwycxestansgrhdcxfgbbinheylcmhlsomhrsvddmisvehhgywttnwlwniojecerettfeuyjylnlybsbtoycsfncsfgoycscstpsoihfxhsjpjljzoyaxtpsotansghhdfzfwgycsiocwnsimpafwytieyktsaegunneyvawkidaojzsrmndmptrygucnlelocpeydtnddmndgugakgdsoxwmpfadgumyesvwssgmpeiymddnsbcmbakkguaddernimwmoxdyta"
                },
                "ur:xid/hdcxuysflgfsmwjseozmhplehywpwdcnfwmtvskkkbtieerpsfmtwegoiysaeeylfsecdsfxhljz": {
                  "pet_name": "Bob",
                  "xid_document": "ur:xid/tpsplftpsplftpsotanshdhdcxuysflgfsmwjseozmhplehywpwdcnfwmtvskkkbtieerpsfmtwegoiysaeeylfsecoyaylstpsotansgylftanshfhdcxtoiniabgotbtltwpfgnbcxlybznngywkfsflbabyamadwmuefgtyjecxmteefxjntansgrhdcxbatpyafttpyabewkcmutihvesklrhytehydavdimwpahbalnnsrsnyfzpkcehpfhoycsfncsfgoycscstpsoiafwjlidoyaxtpsotansghhdfzhlimcmkgkkhdpmvsmtiowezcnemnyapaaxvostosrpluaslaylasmuzmsatsotwdchwlwmpsheclgeltynteyleohdwlhdticwdsahrtsrykseptflosbwtkrhlybwoydntkpmem"
                },
                "ur:xid/hdcxwmkbiywnmkwdlprdjliowtdkprkpbszodnlychyklapdjzrohnwpwecefglolsbsfnpkjony": {
                  "pet_name": "Alice",
                  "xid_document": "ur:xid/tpsplftpsplftpsotanshdhdcxwmkbiywnmkwdlprdjliowtdkprkpbszodnlychyklapdjzrohnwpwecefglolsbsoyaylstpsotansgylftanshfhdcxswkeatmoclaehlpezsprtkntgrparfihgosofmfnlrgltndysabkwlckykimemottansgrhdcxtnhluevohylpdadednfmrsdkcfvovdsfaaadpecllftytbhgmylapkbarsfhdthsoycsfncsfgoycscstpsoihfpjziniaihoyaxtpsotansghhdfzkizesfchbgmylycxcesplsatmelfctwdplbeidjkmklehetntyidasgevachftiyotielsidkomoynskpkknpfuojobyrkbncektdsiateluetctyklrgrpshdhfadfzwkesroaa"
                }
              }
            }
        "#}
        .trim()
        .to_string(),
        _ => panic!("unknown owner"),
    }
}

fn register_tags() {
    bc_components::register_tags();
    bc_envelope::register_tags();
    provenance_mark::register_tags();
}
