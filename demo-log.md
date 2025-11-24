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

│ ALICE_PRVKEYS=ur:crypto-prvkeys/lftansgohdcxwmrhtlueynvwtsehmsqdkoiyhkztfsgyuojecnvwfddnyaykbkvdcwhldrbzrovytansgehdcxclktdatiglurwevaswhfbafezmdneolsgegywnuemuosmhltnlbsvyutehdpfxtewpkovloe
│ ALICE_PUBKEYS=ur:crypto-pubkeys/lftanshfhdcxmspklorfkoztiadrtnuybgyagwtkhgadsgecatfpoehsryrhlbtylyecmsvacsnstansgrhdcxoxcfiasrjeahaewsrtqdvacwfrjoztgmuruttpndsspelntezshdnlfdislglydawluetlly
│ ALICE_OWNER_DOC=ur:xid/tpsplftpsplftpsotanshdhdcxykrdndbysrmessaxcyjseneorlpesngyplimluwdfxdwdwesbynthhpehelgmdwdoyaylrtpsotansgylftanshfhdcxmspklorfkoztiadrtnuybgyagwtkhgadsgecatfpoehsryrhlbtylyecmsvacsnstansgrhdcxoxcfiasrjeahaewsrtqdvacwfrjoztgmuruttpndsspelntezshdnlfdislglydaoycsfncsfgoycscstpsoihfpjziniaihlfoycsfptpsotansgtlftansgohdcxwmrhtlueynvwtsehmsqdkoiyhkztfsgyuojecnvwfddnyaykbkvdcwhldrbzrovytansgehdcxclktdatiglurwevaswhfbafezmdneolsgegywnuemuosmhltnlbsvyutehdpfxteoybstpsotansgmhdcxcsmobdeshnfytepevtpmzmregdcypmfrsfueaxqdmnjnfzjntocflbidloaooevdoyaxtpsotansghhdfzluoxrdknbghyrptpkksotknyaepsiaiapkutmkzmmktdfwgsgohsdkmdwdihzeyazmenpmlsfngdtabtyklrdimhidqzwkeeottkfzmsamfewmetfgldtaidtktsnddatkhfwyim
│ ALICE_SIGNED_DOC=ur:xid/tpsplftpsplftpsotanshdhdcxykrdndbysrmessaxcyjseneorlpesngyplimluwdfxdwdwesbynthhpehelgmdwdoyaylstpsotansgylftanshfhdcxmspklorfkoztiadrtnuybgyagwtkhgadsgecatfpoehsryrhlbtylyecmsvacsnstansgrhdcxoxcfiasrjeahaewsrtqdvacwfrjoztgmuruttpndsspelntezshdnlfdislglydaoycsfncsfgoycscstpsoihfpjziniaihoyaxtpsotansghhdfzaebyemhdplgeuedlidnszeghwkryfepsdaetsrbncarozmvtvwfmkpsplufsswrnsebajoecsrpefxwekifgjorofltojzjoplqzuebtytpdsretpddpsafgoltsldetheaolfso
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

│ BOB_PRVKEYS=ur:crypto-prvkeys/lftansgohdcxlpemwzylpkcauechaswsgagwgwlggrpsaddnbklygasrrygtsglresasbsroingltansgehdcxvatiihahmdbendflmdswkglettlklonsuyolhdnlsbioeelehhmdnntyclswfpjtlrsafwqz
│ BOB_PUBKEYS=ur:crypto-pubkeys/lftanshfhdcxdwnladmyasgukityhhrysatshplybdjtbzfhfrdifwisclfrjsnlgucsaaiafgoytansgrhdcxfrbtvlfhonlptoskflmskojecmfzwsotgemtehdnptutryhpbsjomhrojkaolrgywdtagwfs
│ BOB_OWNER_DOC=ur:xid/tpsplftpsplftpsotanshdhdcxecmuzobeatgwbwwzmdcyckbseozceczozccawtmwgedlghoechlkdirkdpfwtowfoyaylrtpsotansgylftanshfhdcxdwnladmyasgukityhhrysatshplybdjtbzfhfrdifwisclfrjsnlgucsaaiafgoytansgrhdcxfrbtvlfhonlptoskflmskojecmfzwsotgemtehdnptutryhpbsjomhrojkaolrgyoycsfncsfglfoycsfptpsotansgtlftansgohdcxlpemwzylpkcauechaswsgagwgwlggrpsaddnbklygasrrygtsglresasbsroingltansgehdcxvatiihahmdbendflmdswkglettlklonsuyolhdnlsbioeelehhmdnntyclswfpjtoybstpsotansgmhdcxlpvowlcmcpldqdfyzeecwyrnihsgihlafgwptkcwatnehdltieykbbyahfwturlyoycscstpsoiafwjlidoyaxtpsotansghhdfznsfmvyryameytbkttlkbrprhsebbtbswtdjyguoycmbnaeltcwtsfsrhrpuyroahvdemmujyfsytwncmgysagyzstddtlahyjtdwetzcmddipkkemnjzamcsksneatisbzesflca
│ BOB_SIGNED_DOC=ur:xid/tpsplftpsplftpsotanshdhdcxecmuzobeatgwbwwzmdcyckbseozceczozccawtmwgedlghoechlkdirkdpfwtowfoyaylstpsotansgylftanshfhdcxdwnladmyasgukityhhrysatshplybdjtbzfhfrdifwisclfrjsnlgucsaaiafgoytansgrhdcxfrbtvlfhonlptoskflmskojecmfzwsotgemtehdnptutryhpbsjomhrojkaolrgyoycsfncsfgoycscstpsoiafwjlidoyaxtpsotansghhdfzptgmknykbbtptehgryaxbwpslgltfdndlolabacpwsckvtatbnprgaiseoswgyldctprknrssrzedmnyrlhnoneosaflctrlchrdkbryadzektutiemkcfaxettibabwvewsuygw
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

│ CAROL_PRVKEYS=ur:crypto-prvkeys/lftansgohdcxdwurmwwloemhielbcsahbdvotbecjzrtdegwmynehefpttsfgwykbbjtlojyfrwdtansgehdcxdpmkwfbapkpthnrshpfrpkrevtdsdrroisaxdtgauocmaojeyatdkbwplofpfdwdiyuelfkk
│ CAROL_PUBKEYS=ur:crypto-pubkeys/lftanshfhdcxamgmrpbdhgqzidsrtykpaooyfgbepdbntahdeypydmhglapldrmymufrleosjldstansgrhdcxkgcychgrmepabbcxpetlhfsngonldrbgtivyluwkvandzobkhgsnretafmdsrpehiymowmck
│ CAROL_OWNER_DOC=ur:xid/tpsplftpsplftpsotanshdhdcxhpkpplstlaaekkaeahlonyrocptihyueahgafsglhfvsssoxjziydrnytpenlodwoyaylrtpsotansgylftanshfhdcxamgmrpbdhgqzidsrtykpaooyfgbepdbntahdeypydmhglapldrmymufrleosjldstansgrhdcxkgcychgrmepabbcxpetlhfsngonldrbgtivyluwkvandzobkhgsnretafmdsrpehoycsfncsfglfoycsfptpsotansgtlftansgohdcxdwurmwwloemhielbcsahbdvotbecjzrtdegwmynehefpttsfgwykbbjtlojyfrwdtansgehdcxdpmkwfbapkpthnrshpfrpkrevtdsdrroisaxdtgauocmaojeyatdkbwplofpfdwdoybstpsotansgmhdcxutplspmkcsrlhyurpsfdatgmbwftfrmkhgfewelopkssdlhnzeotrewzkptavdutoycscstpsoihfxhsjpjljzoyaxtpsotansghhdfzolfesayafrfwfeknlyrsfwfmatahjykslelslpiejepkyaceeefzfgfrmocwsarpkejnnswkbwdswyaapessdtechpfyvscfahfncscfvdenmuehjzcncmsbzesrkiuokghtdwch
│ CAROL_SIGNED_DOC=ur:xid/tpsplftpsplftpsotanshdhdcxhpkpplstlaaekkaeahlonyrocptihyueahgafsglhfvsssoxjziydrnytpenlodwoyaylstpsotansgylftanshfhdcxamgmrpbdhgqzidsrtykpaooyfgbepdbntahdeypydmhglapldrmymufrleosjldstansgrhdcxkgcychgrmepabbcxpetlhfsngonldrbgtivyluwkvandzobkhgsnretafmdsrpehoycsfncsfgoycscstpsoihfxhsjpjljzoyaxtpsotansghhdfztseylubnbsglkgtolofhferdlonedeiesgqzfegojyteaetbnnmuecmdcxrkcazewzjphtvstktsnbdainstfnoytttycwrlkedeambdbdtiaerpplcmlosglabnplwnwyheatts
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

│ DAN_PRVKEYS=ur:crypto-prvkeys/lftansgohdcxtktooxoxdkihzsdphewpsrhybtlydplkaeskglaswtlruyjzvshkuolgfnbbcyrptansgehdcxhelpspcwqzsblsmedkadfxuyftzcfyiyjncfvtinbbqdlbeyfrdrmhkkaogmfxjpmhhlcwjz
│ DAN_PUBKEYS=ur:crypto-pubkeys/lftanshfhdcxvyveprltisaopljllgkpfyoyrpesrfsfgdptjocfkgvlhkntenfwknhttshyktfdtansgrhdcxtadndsemvatptnentabwhtndcxfgrsgapadyfhssnyamntnsutiodnlfenctfnjtpkfhneby
│ DAN_OWNER_DOC=ur:xid/tpsplftpsplftpsotanshdhdcxrdetsffltamyneztfmksdwgyctcfskwlbglncxossgrponihsrjsptynfhwtksheoyaylrtpsotansgylftanshfhdcxvyveprltisaopljllgkpfyoyrpesrfsfgdptjocfkgvlhkntenfwknhttshyktfdtansgrhdcxtadndsemvatptnentabwhtndcxfgrsgapadyfhssnyamntnsutiodnlfenctfnjtoycsfncsfgoycscstpsoiafyhsjtlfoycsfptpsotansgtlftansgohdcxtktooxoxdkihzsdphewpsrhybtlydplkaeskglaswtlruyjzvshkuolgfnbbcyrptansgehdcxhelpspcwqzsblsmedkadfxuyftzcfyiyjncfvtinbbqdlbeyfrdrmhkkaogmfxjpoybstpsotansgmhdcxhkrnwzoyaaledyjtsewscysslaaxprehaewfdrlbienezcflembbrostlpsoynbyoyaxtpsotansghhdfzbbkbberkbnynlnqzvszebakpwywdcfdnuttehfgtglotsosecxdynnnykgcepsoegrwfeopfpdiagmwnytenjpfwkikkmdspbkatguahgmkpsortwncllylkzemsykadfxeclyae
│ DAN_SIGNED_DOC=ur:xid/tpsplftpsplftpsotanshdhdcxrdetsffltamyneztfmksdwgyctcfskwlbglncxossgrponihsrjsptynfhwtksheoyaylstpsotansgylftanshfhdcxvyveprltisaopljllgkpfyoyrpesrfsfgdptjocfkgvlhkntenfwknhttshyktfdtansgrhdcxtadndsemvatptnentabwhtndcxfgrsgapadyfhssnyamntnsutiodnlfenctfnjtoycsfncsfgoycscstpsoiafyhsjtoyaxtpsotansghhdfzvslyvlgmswetlofwoevdehckdlfgiofrhdkthdspbsvejypavwesaevoeodkkkinfywmcaatlbtacfjylbfxgmkoaesfdklkamrefgurrnchprecwfzsoxhlrktkvtnldeldpkwd
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
│     "xid_document": "ur:xid/tpsplftpsplftpsotanshdhdcxykrdndbysrmessaxcyjseneorlpesngyplimluwdfxdwdwesbynthhpehelgmdwdoyaylrtpsotansgylftanshfhdcxmspklorfkoztiadrtnuybgyagwtkhgadsgecatfpoehsryrhlbtylyecmsvacsnstansgrhdcxoxcfiasrjeahaewsrtqdvacwfrjoztgmuruttpndsspelntezshdnlfdislglydaoycsfncsfgoycscstpsoihfpjziniaihlfoycsfptpsotansgtlftansgohdcxwmrhtlueynvwtsehmsqdkoiyhkztfsgyuojecnvwfddnyaykbkvdcwhldrbzrovytansgehdcxclktdatiglurwevaswhfbafezmdneolsgegywnuemuosmhltnlbsvyutehdpfxteoybstpsotansgmhdcxcsmobdeshnfytepevtpmzmregdcypmfrsfueaxqdmnjnfzjntocflbidloaooevdoyaxtpsotansghhdfzluoxrdknbghyrptpkksotknyaepsiaiapkutmkzmmktdfwgsgohsdkmdwdihzeyazmenpmlsfngdtabtyklrdimhidqzwkeeottkfzmsamfewmetfgldtaidtktsnddatkhfwyim"
│   },
│   "participants": {
│     "ur:xid/hdcxecmuzobeatgwbwwzmdcyckbseozceczozccawtmwgedlghoechlkdirkdpfwtowfptpsrygh": {
│       "xid_document": "ur:xid/tpsplftpsplftpsotanshdhdcxecmuzobeatgwbwwzmdcyckbseozceczozccawtmwgedlghoechlkdirkdpfwtowfoyaylstpsotansgylftanshfhdcxdwnladmyasgukityhhrysatshplybdjtbzfhfrdifwisclfrjsnlgucsaaiafgoytansgrhdcxfrbtvlfhonlptoskflmskojecmfzwsotgemtehdnptutryhpbsjomhrojkaolrgyoycsfncsfgoycscstpsoiafwjlidoyaxtpsotansghhdfzptgmknykbbtptehgryaxbwpslgltfdndlolabacpwsckvtatbnprgaiseoswgyldctprknrssrzedmnyrlhnoneosaflctrlchrdkbryadzektutiemkcfaxettibabwvewsuygw",
│       "pet_name": "Bob"
│     },
│     "ur:xid/hdcxhpkpplstlaaekkaeahlonyrocptihyueahgafsglhfvsssoxjziydrnytpenlodwgwtizopk": {
│       "xid_document": "ur:xid/tpsplftpsplftpsotanshdhdcxhpkpplstlaaekkaeahlonyrocptihyueahgafsglhfvsssoxjziydrnytpenlodwoyaylstpsotansgylftanshfhdcxamgmrpbdhgqzidsrtykpaooyfgbepdbntahdeypydmhglapldrmymufrleosjldstansgrhdcxkgcychgrmepabbcxpetlhfsngonldrbgtivyluwkvandzobkhgsnretafmdsrpehoycsfncsfgoycscstpsoihfxhsjpjljzoyaxtpsotansghhdfztseylubnbsglkgtolofhferdlonedeiesgqzfegojyteaetbnnmuecmdcxrkcazewzjphtvstktsnbdainstfnoytttycwrlkedeambdbdtiaerpplcmlosglabnplwnwyheatts",
│       "pet_name": "Carol"
│     },
│     "ur:xid/hdcxrdetsffltamyneztfmksdwgyctcfskwlbglncxossgrponihsrjsptynfhwtkshevaytropt": {
│       "xid_document": "ur:xid/tpsplftpsplftpsotanshdhdcxrdetsffltamyneztfmksdwgyctcfskwlbglncxossgrponihsrjsptynfhwtksheoyaylstpsotansgylftanshfhdcxvyveprltisaopljllgkpfyoyrpesrfsfgdptjocfkgvlhkntenfwknhttshyktfdtansgrhdcxtadndsemvatptnentabwhtndcxfgrsgapadyfhssnyamntnsutiodnlfenctfnjtoycsfncsfgoycscstpsoiafyhsjtoyaxtpsotansghhdfzvslyvlgmswetlofwoevdehckdlfgiofrhdkthdspbsvejypavwesaevoeodkkkinfywmcaatlbtacfjylbfxgmkoaesfdklkamrefgurrnchprecwfzsoxhlrktkvtnldeldpkwd",
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
│     "xid_document": "ur:xid/tpsplftpsplftpsotanshdhdcxecmuzobeatgwbwwzmdcyckbseozceczozccawtmwgedlghoechlkdirkdpfwtowfoyaylrtpsotansgylftanshfhdcxdwnladmyasgukityhhrysatshplybdjtbzfhfrdifwisclfrjsnlgucsaaiafgoytansgrhdcxfrbtvlfhonlptoskflmskojecmfzwsotgemtehdnptutryhpbsjomhrojkaolrgyoycsfncsfglfoycsfptpsotansgtlftansgohdcxlpemwzylpkcauechaswsgagwgwlggrpsaddnbklygasrrygtsglresasbsroingltansgehdcxvatiihahmdbendflmdswkglettlklonsuyolhdnlsbioeelehhmdnntyclswfpjtoybstpsotansgmhdcxlpvowlcmcpldqdfyzeecwyrnihsgihlafgwptkcwatnehdltieykbbyahfwturlyoycscstpsoiafwjlidoyaxtpsotansghhdfznsfmvyryameytbkttlkbrprhsebbtbswtdjyguoycmbnaeltcwtsfsrhrpuyroahvdemmujyfsytwncmgysagyzstddtlahyjtdwetzcmddipkkemnjzamcsksneatisbzesflca"
│   },
│   "participants": {
│     "ur:xid/hdcxhpkpplstlaaekkaeahlonyrocptihyueahgafsglhfvsssoxjziydrnytpenlodwgwtizopk": {
│       "xid_document": "ur:xid/tpsplftpsplftpsotanshdhdcxhpkpplstlaaekkaeahlonyrocptihyueahgafsglhfvsssoxjziydrnytpenlodwoyaylstpsotansgylftanshfhdcxamgmrpbdhgqzidsrtykpaooyfgbepdbntahdeypydmhglapldrmymufrleosjldstansgrhdcxkgcychgrmepabbcxpetlhfsngonldrbgtivyluwkvandzobkhgsnretafmdsrpehoycsfncsfgoycscstpsoihfxhsjpjljzoyaxtpsotansghhdfztseylubnbsglkgtolofhferdlonedeiesgqzfegojyteaetbnnmuecmdcxrkcazewzjphtvstktsnbdainstfnoytttycwrlkedeambdbdtiaerpplcmlosglabnplwnwyheatts",
│       "pet_name": "Carol"
│     },
│     "ur:xid/hdcxrdetsffltamyneztfmksdwgyctcfskwlbglncxossgrponihsrjsptynfhwtkshevaytropt": {
│       "xid_document": "ur:xid/tpsplftpsplftpsotanshdhdcxrdetsffltamyneztfmksdwgyctcfskwlbglncxossgrponihsrjsptynfhwtksheoyaylstpsotansgylftanshfhdcxvyveprltisaopljllgkpfyoyrpesrfsfgdptjocfkgvlhkntenfwknhttshyktfdtansgrhdcxtadndsemvatptnentabwhtndcxfgrsgapadyfhssnyamntnsutiodnlfenctfnjtoycsfncsfgoycscstpsoiafyhsjtoyaxtpsotansghhdfzvslyvlgmswetlofwoevdehckdlfgiofrhdkthdspbsvejypavwesaevoeodkkkinfywmcaatlbtacfjylbfxgmkoaesfdklkamrefgurrnchprecwfzsoxhlrktkvtnldeldpkwd",
│       "pet_name": "Dan"
│     },
│     "ur:xid/hdcxykrdndbysrmessaxcyjseneorlpesngyplimluwdfxdwdwesbynthhpehelgmdwdrhrpvonn": {
│       "xid_document": "ur:xid/tpsplftpsplftpsotanshdhdcxykrdndbysrmessaxcyjseneorlpesngyplimluwdfxdwdwesbynthhpehelgmdwdoyaylstpsotansgylftanshfhdcxmspklorfkoztiadrtnuybgyagwtkhgadsgecatfpoehsryrhlbtylyecmsvacsnstansgrhdcxoxcfiasrjeahaewsrtqdvacwfrjoztgmuruttpndsspelntezshdnlfdislglydaoycsfncsfgoycscstpsoihfpjziniaihoyaxtpsotansghhdfzaebyemhdplgeuedlidnszeghwkryfepsdaetsrbncarozmvtvwfmkpsplufsswrnsebajoecsrpefxwekifgjorofltojzjoplqzuebtytpdsretpddpsafgoltsldetheaolfso",
│       "pet_name": "Alice"
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
│     "xid_document": "ur:xid/tpsplftpsplftpsotanshdhdcxhpkpplstlaaekkaeahlonyrocptihyueahgafsglhfvsssoxjziydrnytpenlodwoyaylrtpsotansgylftanshfhdcxamgmrpbdhgqzidsrtykpaooyfgbepdbntahdeypydmhglapldrmymufrleosjldstansgrhdcxkgcychgrmepabbcxpetlhfsngonldrbgtivyluwkvandzobkhgsnretafmdsrpehoycsfncsfglfoycsfptpsotansgtlftansgohdcxdwurmwwloemhielbcsahbdvotbecjzrtdegwmynehefpttsfgwykbbjtlojyfrwdtansgehdcxdpmkwfbapkpthnrshpfrpkrevtdsdrroisaxdtgauocmaojeyatdkbwplofpfdwdoybstpsotansgmhdcxutplspmkcsrlhyurpsfdatgmbwftfrmkhgfewelopkssdlhnzeotrewzkptavdutoycscstpsoihfxhsjpjljzoyaxtpsotansghhdfzolfesayafrfwfeknlyrsfwfmatahjykslelslpiejepkyaceeefzfgfrmocwsarpkejnnswkbwdswyaapessdtechpfyvscfahfncscfvdenmuehjzcncmsbzesrkiuokghtdwch"
│   },
│   "participants": {
│     "ur:xid/hdcxecmuzobeatgwbwwzmdcyckbseozceczozccawtmwgedlghoechlkdirkdpfwtowfptpsrygh": {
│       "xid_document": "ur:xid/tpsplftpsplftpsotanshdhdcxecmuzobeatgwbwwzmdcyckbseozceczozccawtmwgedlghoechlkdirkdpfwtowfoyaylstpsotansgylftanshfhdcxdwnladmyasgukityhhrysatshplybdjtbzfhfrdifwisclfrjsnlgucsaaiafgoytansgrhdcxfrbtvlfhonlptoskflmskojecmfzwsotgemtehdnptutryhpbsjomhrojkaolrgyoycsfncsfgoycscstpsoiafwjlidoyaxtpsotansghhdfzptgmknykbbtptehgryaxbwpslgltfdndlolabacpwsckvtatbnprgaiseoswgyldctprknrssrzedmnyrlhnoneosaflctrlchrdkbryadzektutiemkcfaxettibabwvewsuygw",
│       "pet_name": "Bob"
│     },
│     "ur:xid/hdcxrdetsffltamyneztfmksdwgyctcfskwlbglncxossgrponihsrjsptynfhwtkshevaytropt": {
│       "xid_document": "ur:xid/tpsplftpsplftpsotanshdhdcxrdetsffltamyneztfmksdwgyctcfskwlbglncxossgrponihsrjsptynfhwtksheoyaylstpsotansgylftanshfhdcxvyveprltisaopljllgkpfyoyrpesrfsfgdptjocfkgvlhkntenfwknhttshyktfdtansgrhdcxtadndsemvatptnentabwhtndcxfgrsgapadyfhssnyamntnsutiodnlfenctfnjtoycsfncsfgoycscstpsoiafyhsjtoyaxtpsotansghhdfzvslyvlgmswetlofwoevdehckdlfgiofrhdkthdspbsvejypavwesaevoeodkkkinfywmcaatlbtacfjylbfxgmkoaesfdklkamrefgurrnchprecwfzsoxhlrktkvtnldeldpkwd",
│       "pet_name": "Dan"
│     },
│     "ur:xid/hdcxykrdndbysrmessaxcyjseneorlpesngyplimluwdfxdwdwesbynthhpehelgmdwdrhrpvonn": {
│       "xid_document": "ur:xid/tpsplftpsplftpsotanshdhdcxykrdndbysrmessaxcyjseneorlpesngyplimluwdfxdwdwesbynthhpehelgmdwdoyaylstpsotansgylftanshfhdcxmspklorfkoztiadrtnuybgyagwtkhgadsgecatfpoehsryrhlbtylyecmsvacsnstansgrhdcxoxcfiasrjeahaewsrtqdvacwfrjoztgmuruttpndsspelntezshdnlfdislglydaoycsfncsfgoycscstpsoihfpjziniaihoyaxtpsotansghhdfzaebyemhdplgeuedlidnszeghwkryfepsdaetsrbncarozmvtvwfmkpsplufsswrnsebajoecsrpefxwekifgjorofltojzjoplqzuebtytpdsretpddpsafgoltsldetheaolfso",
│       "pet_name": "Alice"
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
│     "xid_document": "ur:xid/tpsplftpsplftpsotanshdhdcxrdetsffltamyneztfmksdwgyctcfskwlbglncxossgrponihsrjsptynfhwtksheoyaylrtpsotansgylftanshfhdcxvyveprltisaopljllgkpfyoyrpesrfsfgdptjocfkgvlhkntenfwknhttshyktfdtansgrhdcxtadndsemvatptnentabwhtndcxfgrsgapadyfhssnyamntnsutiodnlfenctfnjtoycsfncsfgoycscstpsoiafyhsjtlfoycsfptpsotansgtlftansgohdcxtktooxoxdkihzsdphewpsrhybtlydplkaeskglaswtlruyjzvshkuolgfnbbcyrptansgehdcxhelpspcwqzsblsmedkadfxuyftzcfyiyjncfvtinbbqdlbeyfrdrmhkkaogmfxjpoybstpsotansgmhdcxhkrnwzoyaaledyjtsewscysslaaxprehaewfdrlbienezcflembbrostlpsoynbyoyaxtpsotansghhdfzbbkbberkbnynlnqzvszebakpwywdcfdnuttehfgtglotsosecxdynnnykgcepsoegrwfeopfpdiagmwnytenjpfwkikkmdspbkatguahgmkpsortwncllylkzemsykadfxeclyae"
│   },
│   "participants": {
│     "ur:xid/hdcxecmuzobeatgwbwwzmdcyckbseozceczozccawtmwgedlghoechlkdirkdpfwtowfptpsrygh": {
│       "xid_document": "ur:xid/tpsplftpsplftpsotanshdhdcxecmuzobeatgwbwwzmdcyckbseozceczozccawtmwgedlghoechlkdirkdpfwtowfoyaylstpsotansgylftanshfhdcxdwnladmyasgukityhhrysatshplybdjtbzfhfrdifwisclfrjsnlgucsaaiafgoytansgrhdcxfrbtvlfhonlptoskflmskojecmfzwsotgemtehdnptutryhpbsjomhrojkaolrgyoycsfncsfgoycscstpsoiafwjlidoyaxtpsotansghhdfzptgmknykbbtptehgryaxbwpslgltfdndlolabacpwsckvtatbnprgaiseoswgyldctprknrssrzedmnyrlhnoneosaflctrlchrdkbryadzektutiemkcfaxettibabwvewsuygw",
│       "pet_name": "Bob"
│     },
│     "ur:xid/hdcxhpkpplstlaaekkaeahlonyrocptihyueahgafsglhfvsssoxjziydrnytpenlodwgwtizopk": {
│       "xid_document": "ur:xid/tpsplftpsplftpsotanshdhdcxhpkpplstlaaekkaeahlonyrocptihyueahgafsglhfvsssoxjziydrnytpenlodwoyaylstpsotansgylftanshfhdcxamgmrpbdhgqzidsrtykpaooyfgbepdbntahdeypydmhglapldrmymufrleosjldstansgrhdcxkgcychgrmepabbcxpetlhfsngonldrbgtivyluwkvandzobkhgsnretafmdsrpehoycsfncsfgoycscstpsoihfxhsjpjljzoyaxtpsotansghhdfztseylubnbsglkgtolofhferdlonedeiesgqzfegojyteaetbnnmuecmdcxrkcazewzjphtvstktsnbdainstfnoytttycwrlkedeambdbdtiaerpplcmlosglabnplwnwyheatts",
│       "pet_name": "Carol"
│     },
│     "ur:xid/hdcxykrdndbysrmessaxcyjseneorlpesngyplimluwdfxdwdwesbynthhpehelgmdwdrhrpvonn": {
│       "xid_document": "ur:xid/tpsplftpsplftpsotanshdhdcxykrdndbysrmessaxcyjseneorlpesngyplimluwdfxdwdwesbynthhpehelgmdwdoyaylstpsotansgylftanshfhdcxmspklorfkoztiadrtnuybgyagwtkhgadsgecatfpoehsryrhlbtylyecmsvacsnstansgrhdcxoxcfiasrjeahaewsrtqdvacwfrjoztgmuruttpndsspelntezshdnlfdislglydaoycsfncsfgoycscstpsoihfpjziniaihoyaxtpsotansghhdfzaebyemhdplgeuedlidnszeghwkryfepsdaetsrbncarozmvtvwfmkpsplufsswrnsebajoecsrpefxwekifgjorofltojzjoplqzuebtytpdsretpddpsafgoltsldetheaolfso",
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

│ request(ARID(0e7d50a1)) [
│     'body': «"dkgGroupInvite"» [
│         ❰"charter"❱: "This group will authorize new club editions."
│         ❰"minSigners"❱: 2
│         ❰"participant"❱: {
│             {
│                 XID(3593fb10) [
│                     'key': PublicKeys(ff91b859, SigningPublicKey(3593fb10, SchnorrPublicKey(fa815b7c)), EncapsulationPublicKey(8789d127, X25519PublicKey(8789d127))) [
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
│                 XID(5b75aec7) [
│                     'key': PublicKeys(2647e9b2, SigningPublicKey(5b75aec7, SchnorrPublicKey(e3d4a47c)), EncapsulationPublicKey(47f17b26, X25519PublicKey(47f17b26))) [
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
│                 XID(ba38cc47) [
│                     'key': PublicKeys(618c7b20, SigningPublicKey(ba38cc47, SchnorrPublicKey(2d86c964)), EncapsulationPublicKey(f407152b, X25519PublicKey(f407152b))) [
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
│         ❰"session"❱: ARID(0db6a97c)
│         ❰"validUntil"❱: 2025-11-24T06:01:45Z
│     ]
│     'date': 2025-11-24T05:01:45Z
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

