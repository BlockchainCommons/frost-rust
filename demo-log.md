## Set zsh options

zsh is the default shell on macOS and many Linux systems. This keeps history markers out of the transcript.

```
setopt nobanghist
```

## Checking prerequisites

Verify that the required CLI tools are present and available in $PATH.

```
for cmd in frost envelope; do
  $cmd --version
done

│ frost 0.1.0
│ bc-envelope-cli 0.27.0
```

## Preparing demo workspace

Start with a clean directory to capture demo artifacts.

```
rm -rf demo && mkdir -p demo
```

## Provisioning XID for Alice

Generate Alice's key material, a private XID document (for owner use), and a signed public XID document (for participants).

```
ALICE_PRVKEYS=$(envelope generate prvkeys)
echo "ALICE_PRVKEYS=$ALICE_PRVKEYS"
ALICE_PUBKEYS=$(envelope generate pubkeys "$ALICE_PRVKEYS")
echo "ALICE_PUBKEYS=$ALICE_PUBKEYS"
ALICE_OWNER_DOC=$(envelope xid new --nickname Alice --sign inception "$ALICE_PRVKEYS")
echo "ALICE_OWNER_DOC=$ALICE_OWNER_DOC"
ALICE_SIGNED_DOC=$(envelope xid new --nickname Alice --private omit --sign inception "$ALICE_PRVKEYS")
echo "ALICE_SIGNED_DOC=$ALICE_SIGNED_DOC"

│ ALICE_PRVKEYS=ur:crypto-prvkeys/lftansgohdcxzsidrpdmjppduofxswtleyjztsfzdwfyeeolidfscynnnnsogltnsaaatksgststtansgehdcxkiotfdldtalfimytdwskletoflbwimrptdvewmctetbafdmusofswyhtmtcyrtclmdgyvwfz
│ ALICE_PUBKEYS=ur:crypto-pubkeys/lftanshfhdcxmhhfgscswfssihjzpflkidwpmwplskbauefgeobycmambgbtbyndjzfywkoedsbdtansgrhdcxclbyjngtcmtsjkkbsrtblramdszodiolfxcnlgdeplwkahgmhhamgyfwkojezmjzsocmeoge
│ ALICE_OWNER_DOC=ur:xid/tpsplftpsplftpsotanshdhdcxryroolatenktgrsbpeclkoaapyflmeghdwttssmuvtvwrerpsnvdtnhgnblthswkoyaylrtpsotansgylftanshfhdcxmhhfgscswfssihjzpflkidwpmwplskbauefgeobycmambgbtbyndjzfywkoedsbdtansgrhdcxclbyjngtcmtsjkkbsrtblramdszodiolfxcnlgdeplwkahgmhhamgyfwkojezmjzoycsfncsfglfoycsfptpsotansgtlftansgohdcxzsidrpdmjppduofxswtleyjztsfzdwfyeeolidfscynnnnsogltnsaaatksgststtansgehdcxkiotfdldtalfimytdwskletoflbwimrptdvewmctetbafdmusofswyhtmtcyrtcloybstpsotansgmhdcxwpbefpmohdkighonvyjtrehljecsrdtooxdwdnasiemeghmwvdfxhlntzmynbnseoycscstpsoihfpjziniaihoyaxtpsotansghhdfzrpplotfplnmsiospjptbaagdjohndwtppetekgnyfmfpasjyhgpaetdasaylksqzkessisbbademsfwfwylaadvlrtbthkyarojplgdapfoxlklsmonslgmtjtaxcplnvtsbfhqd
│ ALICE_SIGNED_DOC=ur:xid/tpsplftpsplftpsotanshdhdcxryroolatenktgrsbpeclkoaapyflmeghdwttssmuvtvwrerpsnvdtnhgnblthswkoyaylstpsotansgylftanshfhdcxmhhfgscswfssihjzpflkidwpmwplskbauefgeobycmambgbtbyndjzfywkoedsbdtansgrhdcxclbyjngtcmtsjkkbsrtblramdszodiolfxcnlgdeplwkahgmhhamgyfwkojezmjzoycsfncsfgoycscstpsoihfpjziniaihoyaxtpsotansghhdfzfewecpwkpelsmwonvdosrhpywlrsrdpywttenlcpprlgjoldecueoxhllovakoienlbdpkvwhnamtiotbsjooelagudlehzsfttdhhresbtpttgewylbtladadzcaeiskkimjlsw
```

## Provisioning XID for Bob

Generate Bob's key material, a private XID document (for owner use), and a signed public XID document (for participants).

```
BOB_PRVKEYS=$(envelope generate prvkeys)
echo "BOB_PRVKEYS=$BOB_PRVKEYS"
BOB_PUBKEYS=$(envelope generate pubkeys "$BOB_PRVKEYS")
echo "BOB_PUBKEYS=$BOB_PUBKEYS"
BOB_OWNER_DOC=$(envelope xid new --nickname Bob --sign inception "$BOB_PRVKEYS")
echo "BOB_OWNER_DOC=$BOB_OWNER_DOC"
BOB_SIGNED_DOC=$(envelope xid new --nickname Bob --private omit --sign inception "$BOB_PRVKEYS")
echo "BOB_SIGNED_DOC=$BOB_SIGNED_DOC"

│ BOB_PRVKEYS=ur:crypto-prvkeys/lftansgohdcxtnloasmtrhldlaaajscsdpfzsoswktswssnttirftpcystlnflbacttajomwgutltansgehdcxtspaoedrwfosynrkpebshtvtbzrlmkmdaomytodmgtlyyliycplyserllkmssoamwlzchswt
│ BOB_PUBKEYS=ur:crypto-pubkeys/lftanshfhdcxhnvtvewtdnoljyossedmfsbgfmdwploeehwdckmsptuebzplntkicnolyleycpmetansgrhdcxbebdgusbcflygebaehlfbskkcwwpcnpepejtfgvslfmwntlgcnnbskzeeepmpffedaprwdlg
│ BOB_OWNER_DOC=ur:xid/tpsplftpsplftpsotanshdhdcxiadedapfhkfmahjseebklfdmbgbsvahfjewnimotpfvsftdkvakotkbdbgsrrooyoyaylrtpsotansgylftanshfhdcxhnvtvewtdnoljyossedmfsbgfmdwploeehwdckmsptuebzplntkicnolyleycpmetansgrhdcxbebdgusbcflygebaehlfbskkcwwpcnpepejtfgvslfmwntlgcnnbskzeeepmpffeoycsfncsfgoycscstpsoiafwjlidlfoycsfptpsotansgtlftansgohdcxtnloasmtrhldlaaajscsdpfzsoswktswssnttirftpcystlnflbacttajomwgutltansgehdcxtspaoedrwfosynrkpebshtvtbzrlmkmdaomytodmgtlyyliycplyserllkmssoamoybstpsotansgmhdcxjlmkzosedarsdeonpkksclihcfnyhdptlakpwtdsmngdrdcshfhdhppmjpgezcmnoyaxtpsotansghhdfzwsvsfsendnnlbgtswzkkcllfltmwlgvsguhnqdioktvsguprldpsfnwstldsjnksldpsyabztptlkimwfpgeskhgidatatgrskgtcllknbpagmbahthdbzstoxpfbdbntizcnlhf
│ BOB_SIGNED_DOC=ur:xid/tpsplftpsplftpsotanshdhdcxiadedapfhkfmahjseebklfdmbgbsvahfjewnimotpfvsftdkvakotkbdbgsrrooyoyaylstpsotansgylftanshfhdcxhnvtvewtdnoljyossedmfsbgfmdwploeehwdckmsptuebzplntkicnolyleycpmetansgrhdcxbebdgusbcflygebaehlfbskkcwwpcnpepejtfgvslfmwntlgcnnbskzeeepmpffeoycsfncsfgoycscstpsoiafwjlidoyaxtpsotansghhdfzrpdigdimqdaorsyagufszogwhlaswtosmkjehdfhptestbcalesfyndmbkltnsbnmeryamkosfkiheamhfemlnwmjsnbldhlidlrjlfwhgclonjertrodkglaowpfprebtpmvllt
```

## Provisioning XID for Carol

Generate Carol's key material, a private XID document (for owner use), and a signed public XID document (for participants).

```
CAROL_PRVKEYS=$(envelope generate prvkeys)
echo "CAROL_PRVKEYS=$CAROL_PRVKEYS"
CAROL_PUBKEYS=$(envelope generate pubkeys "$CAROL_PRVKEYS")
echo "CAROL_PUBKEYS=$CAROL_PUBKEYS"
CAROL_OWNER_DOC=$(envelope xid new --nickname Carol --sign inception "$CAROL_PRVKEYS")
echo "CAROL_OWNER_DOC=$CAROL_OWNER_DOC"
CAROL_SIGNED_DOC=$(envelope xid new --nickname Carol --private omit --sign inception "$CAROL_PRVKEYS")
echo "CAROL_SIGNED_DOC=$CAROL_SIGNED_DOC"

│ CAROL_PRVKEYS=ur:crypto-prvkeys/lftansgohdcxwtenswmoqzndticxrhwytdfekobwhewyfstykgwsutaalaoxroylkoaxwyftvdhhtansgehdcxmnndpspydinsiodlaymtmdkemwbgeyasfzqdyapetnwnstuekeemksuorocsolswwpbntibz
│ CAROL_PUBKEYS=ur:crypto-pubkeys/lftanshfhdcxhsrfmwgumytybzcecpmhkeqzrtmwredytdrefzpezcjlsffprftnnesbsfntlksrtansgrhdcxpketqdttenprmsbnpfbssrgyuyuyiawzwmgulprondfnctzmnbwmswsbcyaybgiojzcsfree
│ CAROL_OWNER_DOC=ur:xid/tpsplftpsplftpsotanshdhdcxosgrrndtnyvachaehgweflhfzetiolihstfldaiyvtvetacsgraygyldayjklsmnoyaylrtpsotansgylftanshfhdcxhsrfmwgumytybzcecpmhkeqzrtmwredytdrefzpezcjlsffprftnnesbsfntlksrtansgrhdcxpketqdttenprmsbnpfbssrgyuyuyiawzwmgulprondfnctzmnbwmswsbcyaybgiooycsfncsfglfoycsfptpsotansgtlftansgohdcxwtenswmoqzndticxrhwytdfekobwhewyfstykgwsutaalaoxroylkoaxwyftvdhhtansgehdcxmnndpspydinsiodlaymtmdkemwbgeyasfzqdyapetnwnstuekeemksuorocsolswoybstpsotansgmhdcxhhnylftywklkgdjsryvaaeahgudmhyjoatgahylghksoyncswfemktkeayioltqdoycscstpsoihfxhsjpjljzoyaxtpsotansghhdfzseintlzehffwdwpmjnethshkvscwzsrdmtvadtwtvsykoxjeontionehwkclnlndbtoedsaepfktsfntpdfhytlyjpecckwzstaedlldwkcxbyjntsmhcadebabzvwehoymdiogu
│ CAROL_SIGNED_DOC=ur:xid/tpsplftpsplftpsotanshdhdcxosgrrndtnyvachaehgweflhfzetiolihstfldaiyvtvetacsgraygyldayjklsmnoyaylstpsotansgylftanshfhdcxhsrfmwgumytybzcecpmhkeqzrtmwredytdrefzpezcjlsffprftnnesbsfntlksrtansgrhdcxpketqdttenprmsbnpfbssrgyuyuyiawzwmgulprondfnctzmnbwmswsbcyaybgiooycsfncsfgoycscstpsoihfxhsjpjljzoyaxtpsotansghhdfzprkofwkgbgiomdeelyflmuaavypdbgmkgybnhscfehlbfsurmuhfnddljkpyjovllkstdrtpamuttlfplsfeimhyzejshnswwnldtbltnywtrdbdhpencaskglkibwcnbyaeprvy
```

## Provisioning XID for Dan

Generate Dan's key material, a private XID document (for owner use), and a signed public XID document (for participants).

```
DAN_PRVKEYS=$(envelope generate prvkeys)
echo "DAN_PRVKEYS=$DAN_PRVKEYS"
DAN_PUBKEYS=$(envelope generate pubkeys "$DAN_PRVKEYS")
echo "DAN_PUBKEYS=$DAN_PUBKEYS"
DAN_OWNER_DOC=$(envelope xid new --nickname Dan --sign inception "$DAN_PRVKEYS")
echo "DAN_OWNER_DOC=$DAN_OWNER_DOC"
DAN_SIGNED_DOC=$(envelope xid new --nickname Dan --private omit --sign inception "$DAN_PRVKEYS")
echo "DAN_SIGNED_DOC=$DAN_SIGNED_DOC"

│ DAN_PRVKEYS=ur:crypto-prvkeys/lftansgohdcxemknplcycedlfejpinutswaxjsfhjyflstkngoswaagerememkghwkfgpmylpkmwtansgehdcxfntlcldkspjsrpptjlmdtdhpteptuyemdrksjehlftrksogmdpfnesdiatstvahdjzlreowm
│ DAN_PUBKEYS=ur:crypto-pubkeys/lftanshfhdcxlkrokiktwzahmtecceinbakgmyrhjkenwmltlsprwdwtlphljkrdlpzstigyftoetansgrhdcxrfoylthdndglrtbafsnsdtmourbnwyspaswzrycejkkgtlgmwlvstngugrlkiydrspsbfelb
│ DAN_OWNER_DOC=ur:xid/tpsplftpsplftpsotanshdhdcxsbveiyrkladehdectedawmzeryvatpgsgeahahfnbasfotfsmdenfpweckosfshgoyaylrtpsotansgylftanshfhdcxlkrokiktwzahmtecceinbakgmyrhjkenwmltlsprwdwtlphljkrdlpzstigyftoetansgrhdcxrfoylthdndglrtbafsnsdtmourbnwyspaswzrycejkkgtlgmwlvstngugrlkiydrlfoycsfptpsotansgtlftansgohdcxemknplcycedlfejpinutswaxjsfhjyflstkngoswaagerememkghwkfgpmylpkmwtansgehdcxfntlcldkspjsrpptjlmdtdhpteptuyemdrksjehlftrksogmdpfnesdiatstvahdoybstpsotansgmhdcxmuceprldrewzatmnrshlehteeymonyadgawsjpfsjychwdatztspolwytbpynstyoycsfncsfgoycscstpsoiafyhsjtoyaxtpsotansghhdfzaxdnaacaadihryjswplfrhnttkgmlrcynbamensoahaeeeoxiyvsfgaogmbkmeayjssfayecsapecnqdtdrfgrsrroeogyaapygwayzepmfrceftsnqdvowylskgmkhpgddtldcl
│ DAN_SIGNED_DOC=ur:xid/tpsplftpsplftpsotanshdhdcxsbveiyrkladehdectedawmzeryvatpgsgeahahfnbasfotfsmdenfpweckosfshgoyaylstpsotansgylftanshfhdcxlkrokiktwzahmtecceinbakgmyrhjkenwmltlsprwdwtlphljkrdlpzstigyftoetansgrhdcxrfoylthdndglrtbafsnsdtmourbnwyspaswzrycejkkgtlgmwlvstngugrlkiydroycsfncsfgoycscstpsoiafyhsjtoyaxtpsotansghhdfzbnzokiqzgmfelawlmkgtvefpethtpsdwjpztkbcpgemofepsprureoamchndjtfryalngegmfmhgrflkcspdjzdwvwpdgsghlepkcnksaatpgapmemehbnrdpaaxtlgelyteylvd
```

## Building Alice's registry

Set Alice as the registry owner using the private XID document, then add the other three participants with their signed XID documents.

```
ALICE_REGISTRY=demo/alice-registry.json
frost registry owner set --registry "$ALICE_REGISTRY" "$ALICE_OWNER_DOC"
frost registry participant add --registry "$ALICE_REGISTRY" "$BOB_SIGNED_DOC" Bob
frost registry participant add --registry "$ALICE_REGISTRY" "$CAROL_SIGNED_DOC" Carol
frost registry participant add --registry "$ALICE_REGISTRY" "$DAN_SIGNED_DOC" Dan
cat "$ALICE_REGISTRY"

│ {
│   "owner": {
│     "xid_document": "ur:xid/tpsplftpsplftpsotanshdhdcxryroolatenktgrsbpeclkoaapyflmeghdwttssmuvtvwrerpsnvdtnhgnblthswkoyaylrtpsotansgylftanshfhdcxmhhfgscswfssihjzpflkidwpmwplskbauefgeobycmambgbtbyndjzfywkoedsbdtansgrhdcxclbyjngtcmtsjkkbsrtblramdszodiolfxcnlgdeplwkahgmhhamgyfwkojezmjzoycsfncsfglfoycsfptpsotansgtlftansgohdcxzsidrpdmjppduofxswtleyjztsfzdwfyeeolidfscynnnnsogltnsaaatksgststtansgehdcxkiotfdldtalfimytdwskletoflbwimrptdvewmctetbafdmusofswyhtmtcyrtcloybstpsotansgmhdcxwpbefpmohdkighonvyjtrehljecsrdtooxdwdnasiemeghmwvdfxhlntzmynbnseoycscstpsoihfpjziniaihoyaxtpsotansghhdfzrpplotfplnmsiospjptbaagdjohndwtppetekgnyfmfpasjyhgpaetdasaylksqzkessisbbademsfwfwylaadvlrtbthkyarojplgdapfoxlklsmonslgmtjtaxcplnvtsbfhqd"
│   },
│   "participants": {
│     "ur:xid/hdcxiadedapfhkfmahjseebklfdmbgbsvahfjewnimotpfvsftdkvakotkbdbgsrrooyplrdstvl": {
│       "xid_document": "ur:xid/tpsplftpsplftpsotanshdhdcxiadedapfhkfmahjseebklfdmbgbsvahfjewnimotpfvsftdkvakotkbdbgsrrooyoyaylstpsotansgylftanshfhdcxhnvtvewtdnoljyossedmfsbgfmdwploeehwdckmsptuebzplntkicnolyleycpmetansgrhdcxbebdgusbcflygebaehlfbskkcwwpcnpepejtfgvslfmwntlgcnnbskzeeepmpffeoycsfncsfgoycscstpsoiafwjlidoyaxtpsotansghhdfzrpdigdimqdaorsyagufszogwhlaswtosmkjehdfhptestbcalesfyndmbkltnsbnmeryamkosfkiheamhfemlnwmjsnbldhlidlrjlfwhgclonjertrodkglaowpfprebtpmvllt",
│       "pet_name": "Bob"
│     },
│     "ur:xid/hdcxosgrrndtnyvachaehgweflhfzetiolihstfldaiyvtvetacsgraygyldayjklsmnbebwtoty": {
│       "xid_document": "ur:xid/tpsplftpsplftpsotanshdhdcxosgrrndtnyvachaehgweflhfzetiolihstfldaiyvtvetacsgraygyldayjklsmnoyaylstpsotansgylftanshfhdcxhsrfmwgumytybzcecpmhkeqzrtmwredytdrefzpezcjlsffprftnnesbsfntlksrtansgrhdcxpketqdttenprmsbnpfbssrgyuyuyiawzwmgulprondfnctzmnbwmswsbcyaybgiooycsfncsfgoycscstpsoihfxhsjpjljzoyaxtpsotansghhdfzprkofwkgbgiomdeelyflmuaavypdbgmkgybnhscfehlbfsurmuhfnddljkpyjovllkstdrtpamuttlfplsfeimhyzejshnswwnldtbltnywtrdbdhpencaskglkibwcnbyaeprvy",
│       "pet_name": "Carol"
│     },
│     "ur:xid/hdcxsbveiyrkladehdectedawmzeryvatpgsgeahahfnbasfotfsmdenfpweckosfshggaftamto": {
│       "xid_document": "ur:xid/tpsplftpsplftpsotanshdhdcxsbveiyrkladehdectedawmzeryvatpgsgeahahfnbasfotfsmdenfpweckosfshgoyaylstpsotansgylftanshfhdcxlkrokiktwzahmtecceinbakgmyrhjkenwmltlsprwdwtlphljkrdlpzstigyftoetansgrhdcxrfoylthdndglrtbafsnsdtmourbnwyspaswzrycejkkgtlgmwlvstngugrlkiydroycsfncsfgoycscstpsoiafyhsjtoyaxtpsotansghhdfzbnzokiqzgmfelawlmkgtvefpethtpsdwjpztkbcpgemofepsprureoamchndjtfryalngegmfmhgrflkcspdjzdwvwpdgsghlepkcnksaatpgapmemehbnrdpaaxtlgelyteylvd",
│       "pet_name": "Dan"
│     }
│   }
│ }
```

## Building Bob's registry

Set Bob as the registry owner using the private XID document, then add the other three participants with their signed XID documents.

```
BOB_REGISTRY=demo/bob-registry.json
frost registry owner set --registry "$BOB_REGISTRY" "$BOB_OWNER_DOC"
frost registry participant add --registry "$BOB_REGISTRY" "$ALICE_SIGNED_DOC" Alice
frost registry participant add --registry "$BOB_REGISTRY" "$CAROL_SIGNED_DOC" Carol
frost registry participant add --registry "$BOB_REGISTRY" "$DAN_SIGNED_DOC" Dan
cat "$BOB_REGISTRY"

│ {
│   "owner": {
│     "xid_document": "ur:xid/tpsplftpsplftpsotanshdhdcxiadedapfhkfmahjseebklfdmbgbsvahfjewnimotpfvsftdkvakotkbdbgsrrooyoyaylrtpsotansgylftanshfhdcxhnvtvewtdnoljyossedmfsbgfmdwploeehwdckmsptuebzplntkicnolyleycpmetansgrhdcxbebdgusbcflygebaehlfbskkcwwpcnpepejtfgvslfmwntlgcnnbskzeeepmpffeoycsfncsfgoycscstpsoiafwjlidlfoycsfptpsotansgtlftansgohdcxtnloasmtrhldlaaajscsdpfzsoswktswssnttirftpcystlnflbacttajomwgutltansgehdcxtspaoedrwfosynrkpebshtvtbzrlmkmdaomytodmgtlyyliycplyserllkmssoamoybstpsotansgmhdcxjlmkzosedarsdeonpkksclihcfnyhdptlakpwtdsmngdrdcshfhdhppmjpgezcmnoyaxtpsotansghhdfzwsvsfsendnnlbgtswzkkcllfltmwlgvsguhnqdioktvsguprldpsfnwstldsjnksldpsyabztptlkimwfpgeskhgidatatgrskgtcllknbpagmbahthdbzstoxpfbdbntizcnlhf"
│   },
│   "participants": {
│     "ur:xid/hdcxosgrrndtnyvachaehgweflhfzetiolihstfldaiyvtvetacsgraygyldayjklsmnbebwtoty": {
│       "xid_document": "ur:xid/tpsplftpsplftpsotanshdhdcxosgrrndtnyvachaehgweflhfzetiolihstfldaiyvtvetacsgraygyldayjklsmnoyaylstpsotansgylftanshfhdcxhsrfmwgumytybzcecpmhkeqzrtmwredytdrefzpezcjlsffprftnnesbsfntlksrtansgrhdcxpketqdttenprmsbnpfbssrgyuyuyiawzwmgulprondfnctzmnbwmswsbcyaybgiooycsfncsfgoycscstpsoihfxhsjpjljzoyaxtpsotansghhdfzprkofwkgbgiomdeelyflmuaavypdbgmkgybnhscfehlbfsurmuhfnddljkpyjovllkstdrtpamuttlfplsfeimhyzejshnswwnldtbltnywtrdbdhpencaskglkibwcnbyaeprvy",
│       "pet_name": "Carol"
│     },
│     "ur:xid/hdcxryroolatenktgrsbpeclkoaapyflmeghdwttssmuvtvwrerpsnvdtnhgnblthswkvyzogrty": {
│       "xid_document": "ur:xid/tpsplftpsplftpsotanshdhdcxryroolatenktgrsbpeclkoaapyflmeghdwttssmuvtvwrerpsnvdtnhgnblthswkoyaylstpsotansgylftanshfhdcxmhhfgscswfssihjzpflkidwpmwplskbauefgeobycmambgbtbyndjzfywkoedsbdtansgrhdcxclbyjngtcmtsjkkbsrtblramdszodiolfxcnlgdeplwkahgmhhamgyfwkojezmjzoycsfncsfgoycscstpsoihfpjziniaihoyaxtpsotansghhdfzfewecpwkpelsmwonvdosrhpywlrsrdpywttenlcpprlgjoldecueoxhllovakoienlbdpkvwhnamtiotbsjooelagudlehzsfttdhhresbtpttgewylbtladadzcaeiskkimjlsw",
│       "pet_name": "Alice"
│     },
│     "ur:xid/hdcxsbveiyrkladehdectedawmzeryvatpgsgeahahfnbasfotfsmdenfpweckosfshggaftamto": {
│       "xid_document": "ur:xid/tpsplftpsplftpsotanshdhdcxsbveiyrkladehdectedawmzeryvatpgsgeahahfnbasfotfsmdenfpweckosfshgoyaylstpsotansgylftanshfhdcxlkrokiktwzahmtecceinbakgmyrhjkenwmltlsprwdwtlphljkrdlpzstigyftoetansgrhdcxrfoylthdndglrtbafsnsdtmourbnwyspaswzrycejkkgtlgmwlvstngugrlkiydroycsfncsfgoycscstpsoiafyhsjtoyaxtpsotansghhdfzbnzokiqzgmfelawlmkgtvefpethtpsdwjpztkbcpgemofepsprureoamchndjtfryalngegmfmhgrflkcspdjzdwvwpdgsghlepkcnksaatpgapmemehbnrdpaaxtlgelyteylvd",
│       "pet_name": "Dan"
│     }
│   }
│ }
```

## Building Carol's registry

Set Carol as the registry owner using the private XID document, then add the other three participants with their signed XID documents.

```
CAROL_REGISTRY=demo/carol-registry.json
frost registry owner set --registry "$CAROL_REGISTRY" "$CAROL_OWNER_DOC"
frost registry participant add --registry "$CAROL_REGISTRY" "$ALICE_SIGNED_DOC" Alice
frost registry participant add --registry "$CAROL_REGISTRY" "$BOB_SIGNED_DOC" Bob
frost registry participant add --registry "$CAROL_REGISTRY" "$DAN_SIGNED_DOC" Dan
cat "$CAROL_REGISTRY"

│ {
│   "owner": {
│     "xid_document": "ur:xid/tpsplftpsplftpsotanshdhdcxosgrrndtnyvachaehgweflhfzetiolihstfldaiyvtvetacsgraygyldayjklsmnoyaylrtpsotansgylftanshfhdcxhsrfmwgumytybzcecpmhkeqzrtmwredytdrefzpezcjlsffprftnnesbsfntlksrtansgrhdcxpketqdttenprmsbnpfbssrgyuyuyiawzwmgulprondfnctzmnbwmswsbcyaybgiooycsfncsfglfoycsfptpsotansgtlftansgohdcxwtenswmoqzndticxrhwytdfekobwhewyfstykgwsutaalaoxroylkoaxwyftvdhhtansgehdcxmnndpspydinsiodlaymtmdkemwbgeyasfzqdyapetnwnstuekeemksuorocsolswoybstpsotansgmhdcxhhnylftywklkgdjsryvaaeahgudmhyjoatgahylghksoyncswfemktkeayioltqdoycscstpsoihfxhsjpjljzoyaxtpsotansghhdfzseintlzehffwdwpmjnethshkvscwzsrdmtvadtwtvsykoxjeontionehwkclnlndbtoedsaepfktsfntpdfhytlyjpecckwzstaedlldwkcxbyjntsmhcadebabzvwehoymdiogu"
│   },
│   "participants": {
│     "ur:xid/hdcxiadedapfhkfmahjseebklfdmbgbsvahfjewnimotpfvsftdkvakotkbdbgsrrooyplrdstvl": {
│       "xid_document": "ur:xid/tpsplftpsplftpsotanshdhdcxiadedapfhkfmahjseebklfdmbgbsvahfjewnimotpfvsftdkvakotkbdbgsrrooyoyaylstpsotansgylftanshfhdcxhnvtvewtdnoljyossedmfsbgfmdwploeehwdckmsptuebzplntkicnolyleycpmetansgrhdcxbebdgusbcflygebaehlfbskkcwwpcnpepejtfgvslfmwntlgcnnbskzeeepmpffeoycsfncsfgoycscstpsoiafwjlidoyaxtpsotansghhdfzrpdigdimqdaorsyagufszogwhlaswtosmkjehdfhptestbcalesfyndmbkltnsbnmeryamkosfkiheamhfemlnwmjsnbldhlidlrjlfwhgclonjertrodkglaowpfprebtpmvllt",
│       "pet_name": "Bob"
│     },
│     "ur:xid/hdcxryroolatenktgrsbpeclkoaapyflmeghdwttssmuvtvwrerpsnvdtnhgnblthswkvyzogrty": {
│       "xid_document": "ur:xid/tpsplftpsplftpsotanshdhdcxryroolatenktgrsbpeclkoaapyflmeghdwttssmuvtvwrerpsnvdtnhgnblthswkoyaylstpsotansgylftanshfhdcxmhhfgscswfssihjzpflkidwpmwplskbauefgeobycmambgbtbyndjzfywkoedsbdtansgrhdcxclbyjngtcmtsjkkbsrtblramdszodiolfxcnlgdeplwkahgmhhamgyfwkojezmjzoycsfncsfgoycscstpsoihfpjziniaihoyaxtpsotansghhdfzfewecpwkpelsmwonvdosrhpywlrsrdpywttenlcpprlgjoldecueoxhllovakoienlbdpkvwhnamtiotbsjooelagudlehzsfttdhhresbtpttgewylbtladadzcaeiskkimjlsw",
│       "pet_name": "Alice"
│     },
│     "ur:xid/hdcxsbveiyrkladehdectedawmzeryvatpgsgeahahfnbasfotfsmdenfpweckosfshggaftamto": {
│       "xid_document": "ur:xid/tpsplftpsplftpsotanshdhdcxsbveiyrkladehdectedawmzeryvatpgsgeahahfnbasfotfsmdenfpweckosfshgoyaylstpsotansgylftanshfhdcxlkrokiktwzahmtecceinbakgmyrhjkenwmltlsprwdwtlphljkrdlpzstigyftoetansgrhdcxrfoylthdndglrtbafsnsdtmourbnwyspaswzrycejkkgtlgmwlvstngugrlkiydroycsfncsfgoycscstpsoiafyhsjtoyaxtpsotansghhdfzbnzokiqzgmfelawlmkgtvefpethtpsdwjpztkbcpgemofepsprureoamchndjtfryalngegmfmhgrflkcspdjzdwvwpdgsghlepkcnksaatpgapmemehbnrdpaaxtlgelyteylvd",
│       "pet_name": "Dan"
│     }
│   }
│ }
```

## Building Dan's registry

Set Dan as the registry owner using the private XID document, then add the other three participants with their signed XID documents.

```
DAN_REGISTRY=demo/dan-registry.json
frost registry owner set --registry "$DAN_REGISTRY" "$DAN_OWNER_DOC"
frost registry participant add --registry "$DAN_REGISTRY" "$ALICE_SIGNED_DOC" Alice
frost registry participant add --registry "$DAN_REGISTRY" "$BOB_SIGNED_DOC" Bob
frost registry participant add --registry "$DAN_REGISTRY" "$CAROL_SIGNED_DOC" Carol
cat "$DAN_REGISTRY"

│ {
│   "owner": {
│     "xid_document": "ur:xid/tpsplftpsplftpsotanshdhdcxsbveiyrkladehdectedawmzeryvatpgsgeahahfnbasfotfsmdenfpweckosfshgoyaylrtpsotansgylftanshfhdcxlkrokiktwzahmtecceinbakgmyrhjkenwmltlsprwdwtlphljkrdlpzstigyftoetansgrhdcxrfoylthdndglrtbafsnsdtmourbnwyspaswzrycejkkgtlgmwlvstngugrlkiydrlfoycsfptpsotansgtlftansgohdcxemknplcycedlfejpinutswaxjsfhjyflstkngoswaagerememkghwkfgpmylpkmwtansgehdcxfntlcldkspjsrpptjlmdtdhpteptuyemdrksjehlftrksogmdpfnesdiatstvahdoybstpsotansgmhdcxmuceprldrewzatmnrshlehteeymonyadgawsjpfsjychwdatztspolwytbpynstyoycsfncsfgoycscstpsoiafyhsjtoyaxtpsotansghhdfzaxdnaacaadihryjswplfrhnttkgmlrcynbamensoahaeeeoxiyvsfgaogmbkmeayjssfayecsapecnqdtdrfgrsrroeogyaapygwayzepmfrceftsnqdvowylskgmkhpgddtldcl"
│   },
│   "participants": {
│     "ur:xid/hdcxiadedapfhkfmahjseebklfdmbgbsvahfjewnimotpfvsftdkvakotkbdbgsrrooyplrdstvl": {
│       "xid_document": "ur:xid/tpsplftpsplftpsotanshdhdcxiadedapfhkfmahjseebklfdmbgbsvahfjewnimotpfvsftdkvakotkbdbgsrrooyoyaylstpsotansgylftanshfhdcxhnvtvewtdnoljyossedmfsbgfmdwploeehwdckmsptuebzplntkicnolyleycpmetansgrhdcxbebdgusbcflygebaehlfbskkcwwpcnpepejtfgvslfmwntlgcnnbskzeeepmpffeoycsfncsfgoycscstpsoiafwjlidoyaxtpsotansghhdfzrpdigdimqdaorsyagufszogwhlaswtosmkjehdfhptestbcalesfyndmbkltnsbnmeryamkosfkiheamhfemlnwmjsnbldhlidlrjlfwhgclonjertrodkglaowpfprebtpmvllt",
│       "pet_name": "Bob"
│     },
│     "ur:xid/hdcxosgrrndtnyvachaehgweflhfzetiolihstfldaiyvtvetacsgraygyldayjklsmnbebwtoty": {
│       "xid_document": "ur:xid/tpsplftpsplftpsotanshdhdcxosgrrndtnyvachaehgweflhfzetiolihstfldaiyvtvetacsgraygyldayjklsmnoyaylstpsotansgylftanshfhdcxhsrfmwgumytybzcecpmhkeqzrtmwredytdrefzpezcjlsffprftnnesbsfntlksrtansgrhdcxpketqdttenprmsbnpfbssrgyuyuyiawzwmgulprondfnctzmnbwmswsbcyaybgiooycsfncsfgoycscstpsoihfxhsjpjljzoyaxtpsotansghhdfzprkofwkgbgiomdeelyflmuaavypdbgmkgybnhscfehlbfsurmuhfnddljkpyjovllkstdrtpamuttlfplsfeimhyzejshnswwnldtbltnywtrdbdhpencaskglkibwcnbyaeprvy",
│       "pet_name": "Carol"
│     },
│     "ur:xid/hdcxryroolatenktgrsbpeclkoaapyflmeghdwttssmuvtvwrerpsnvdtnhgnblthswkvyzogrty": {
│       "xid_document": "ur:xid/tpsplftpsplftpsotanshdhdcxryroolatenktgrsbpeclkoaapyflmeghdwttssmuvtvwrerpsnvdtnhgnblthswkoyaylstpsotansgylftanshfhdcxmhhfgscswfssihjzpflkidwpmwplskbauefgeobycmambgbtbyndjzfywkoedsbdtansgrhdcxclbyjngtcmtsjkkbsrtblramdszodiolfxcnlgdeplwkahgmhhamgyfwkojezmjzoycsfncsfgoycscstpsoihfpjziniaihoyaxtpsotansghhdfzfewecpwkpelsmwonvdosrhpywlrsrdpywttenlcpprlgjoldecueoxhllovakoienlbdpkvwhnamtiotbsjooelagudlehzsfttdhhresbtpttgewylbtladadzcaeiskkimjlsw",
│       "pet_name": "Alice"
│     }
│   }
│ }
```

## Showing Alice's DKG invite (request envelope)

Create a 2-of-3 DKG invite for Bob, Carol, and Dan (from Alice's registry) and format the request envelope to inspect its structure.

```
ALICE_INVITE=$(frost dkg invite show --registry demo/alice-registry.json --min-signers 2 --charter "This group will authorize new club editions." Bob Carol Dan)
echo "${ALICE_INVITE}" | envelope format

│ request(ARID(01d3bdf5)) [
│     'body': «"dkgGroupInvite"» [
│         ❰"charter"❱: "This group will authorize new club editions."
│         ❰"minSigners"❱: 2
│         ❰"participant"❱: {
│             {
│                 XID(632825b0) [
│                     'key': PublicKeys(9fb4963a, SigningPublicKey(632825b0, SchnorrPublicKey(e7ac4d16)), EncapsulationPublicKey(2cb409f9, X25519PublicKey(2cb409f9))) [
│                         'allow': 'All'
│                         'nickname': "Bob"
│                     ]
│                 ]
│             } [
│                 'signed': Signature
│             ]
│         } [
│             "response_arid": ENCRYPTED [
│                 'hasRecipient': SealedMessage
│             ]
│         ]
│         ❰"participant"❱: {
│             {
│                 XID(a74bbe29) [
│                     'key': PublicKeys(d8d1c4d0, SigningPublicKey(a74bbe29, SchnorrPublicKey(6d7b98ef)), EncapsulationPublicKey(c3e2fd94, X25519PublicKey(c3e2fd94))) [
│                         'allow': 'All'
│                         'nickname': "Carol"
│                     ]
│                 ]
│             } [
│                 'signed': Signature
│             ]
│         } [
│             "response_arid": ENCRYPTED [
│                 'hasRecipient': SealedMessage
│             ]
│         ]
│         ❰"participant"❱: {
│             {
│                 XID(cbe466bb) [
│                     'key': PublicKeys(f4476481, SigningPublicKey(cbe466bb, SchnorrPublicKey(875bceec)), EncapsulationPublicKey(8d7be999, X25519PublicKey(8d7be999))) [
│                         'allow': 'All'
│                         'nickname': "Dan"
│                     ]
│                 ]
│             } [
│                 'signed': Signature
│             ]
│         } [
│             "response_arid": ENCRYPTED [
│                 'hasRecipient': SealedMessage
│             ]
│         ]
│         ❰"session"❱: ARID(ecd66ff8)
│         ❰"validUntil"❱: 2025-11-24T08:40:00Z
│     ]
│     'date': 2025-11-24T07:40:00Z
│ ]
```

## Showing Alice's sealed DKG invite

Seal the 2-of-3 invite for Bob, Carol, and Dan and format the sealed envelope to view the encrypted recipient entries.

```
ALICE_INVITE_SEALED=$(frost dkg invite show --registry demo/alice-registry.json --sealed --min-signers 2 --charter "This group will authorize new club editions." Bob Carol Dan)
echo "${ALICE_INVITE_SEALED}" | envelope format
echo "${ALICE_INVITE_SEALED}" | envelope info

│ ENCRYPTED [
│     'hasRecipient': SealedMessage
│     'hasRecipient': SealedMessage
│     'hasRecipient': SealedMessage
│ ]
│ Format: ur:envelope
│ CBOR Size: 2626
│ Description: Gordian Envelope
```

## Checking Hubert server availability

Verify the local Hubert server is responding before publishing the invite.

```
frost check --storage server

│ ✓ Hubert server is available at 127.0.0.1:45678 (version 0.3.0)
```

## Posting sealed DKG invite to Hubert

Seal the invite and store it in Hubert using the default server backend; the printed ARID (UR) can be shared out-of-band.

```
ALICE_INVITE_ARID=$(frost dkg invite put --storage server --registry demo/alice-registry.json --min-signers 2 --charter "This group will authorize new club editions." Bob Carol Dan)
echo "${ALICE_INVITE_ARID}"

│ ur:arid/hdcxwtjzmyftzspmmwpdbncyosknzchevlmdtskolkiolkrhmwregsfmzcldjzkomojkfdfsrsrl
```

## Viewing invite from Hubert as Bob

Retrieve the invite from Hubert using Bob's registry, verify Alice as sender, and show the reply ARID for Bob.

```
frost dkg invite view --storage server --registry demo/bob-registry.json "${ALICE_INVITE_ARID}" Alice

│ Charter: This group will authorize new club editions.
│ Min signers: 2
│ Participants: Carol, ur:xid/hdcxiadedapfhkfmahjseebklfdmbgbsvahfjewnimotpfvsftdkvakotkbdbgsrrooyplrdstvl, Dan
│ Reply ARID: ur:arid/hdcxcxkpdmjysevtdrmkttvejtmddpvyjygylbsewtiysrynfgvsvamukktntyvoryrtplrsgmfm
```

