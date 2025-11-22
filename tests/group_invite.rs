mod common;

use std::time::Duration;

use bc_components::{ARID, PrivateKeyBase, XIDProvider};
use bc_envelope::prelude::*;
use bc_rand::{RandomNumberGenerator, make_fake_random_number_generator};
use bc_xid::{
    XIDDocument, XIDGeneratorOptions, XIDGenesisMarkOptions,
    XIDInceptionKeyOptions, XIDPrivateKeyOptions, XIDSigningOptions,
};
use frost::{DkgGroupInvite, DkgInvitation};
use gstp::SealedRequestBehavior;
use indoc::indoc;
use provenance_mark::ProvenanceMarkResolution;

fn make_xid_document(
    rng: &mut impl RandomNumberGenerator,
    date: Date,
) -> XIDDocument {
    let private_key_base = PrivateKeyBase::new_using(rng);

    XIDDocument::new(
        XIDInceptionKeyOptions::PrivateKeyBase(private_key_base),
        XIDGenesisMarkOptions::Passphrase(
            "password".to_string(),
            Some(ProvenanceMarkResolution::Quartile),
            Some(date),
            None,
        ),
    )
}

fn make_arid(rng: &mut impl RandomNumberGenerator) -> ARID {
    let bytes = rng.random_data(ARID::ARID_SIZE);
    ARID::from_data_ref(bytes).unwrap()
}

#[test]
fn test_dkg_group_invite() {
    provenance_mark::register_tags();

    let mut rng = make_fake_random_number_generator();

    let date = Date::from_ymd(2025, 12, 31);

    let coordinator = make_xid_document(&mut rng, date);

    let alice = make_xid_document(&mut rng, date);
    let bob = make_xid_document(&mut rng, date);
    let carol = make_xid_document(&mut rng, date);
    let min_signers = 2;
    let charter = "Test charter".to_string();

    let alice_ur = alice
        .clone()
        .to_envelope(
            XIDPrivateKeyOptions::default(),
            XIDGeneratorOptions::default(),
            XIDSigningOptions::Inception,
        )
        .unwrap()
        .ur_string();
    let bob_ur = bob
        .clone()
        .to_envelope(
            XIDPrivateKeyOptions::default(),
            XIDGeneratorOptions::default(),
            XIDSigningOptions::Inception,
        )
        .unwrap()
        .ur_string();
    let carol_ur = carol
        .clone()
        .to_envelope(
            XIDPrivateKeyOptions::default(),
            XIDGeneratorOptions::default(),
            XIDSigningOptions::Inception,
        )
        .unwrap()
        .ur_string();

    let participants = vec![alice_ur, bob_ur, carol_ur];

    let request_id = make_arid(&mut rng);
    let session_id = make_arid(&mut rng);
    let expiry = date + Duration::from_secs(7 * 24 * 60 * 60);
    let alice_response_arid = make_arid(&mut rng);
    let bob_response_arid = make_arid(&mut rng);
    let carol_response_arid = make_arid(&mut rng);
    let response_arids =
        vec![alice_response_arid, bob_response_arid, carol_response_arid];
    let invite = DkgGroupInvite::new(
        request_id,
        coordinator.clone(),
        session_id,
        date,
        expiry,
        min_signers,
        charter.clone(),
        participants,
        response_arids,
    )
    .unwrap();

    #[rustfmt::skip]
    let expected_format = (indoc! {r#"
        request(ARID(bbc88f5e)) [
            'body': «"dkgGroupInvite"» [
                ❰"charter"❱: "Test charter"
                ❰"minSigners"❱: 2
                ❰"participant"❱: {
                    {
                        XID(0025f285) [
                            'key': PublicKeys(9e98a427, SigningPublicKey(0025f285, SchnorrPublicKey(3889bdb5)), EncapsulationPublicKey(b4dddc91, X25519PublicKey(b4dddc91))) [
                                'allow': 'All'
                            ]
                            'provenance': ProvenanceMark(59357d99)
                        ]
                    } [
                        'signed': Signature
                    ]
                } [
                    "response_arid": ENCRYPTED [
                        'hasRecipient': SealedMessage
                    ]
                ]
                ❰"participant"❱: {
                    {
                        XID(7c30cafe) [
                            'key': PublicKeys(b8164d99, SigningPublicKey(7c30cafe, SchnorrPublicKey(448e2868)), EncapsulationPublicKey(e472f495, X25519PublicKey(e472f495))) [
                                'allow': 'All'
                            ]
                            'provenance': ProvenanceMark(59357d99)
                        ]
                    } [
                        'signed': Signature
                    ]
                } [
                    "response_arid": ENCRYPTED [
                        'hasRecipient': SealedMessage
                    ]
                ]
                ❰"participant"❱: {
                    {
                        XID(8f188e4f) [
                            'key': PublicKeys(aa29ec7b, SigningPublicKey(8f188e4f, SchnorrPublicKey(71b11348)), EncapsulationPublicKey(91246d82, X25519PublicKey(91246d82))) [
                                'allow': 'All'
                            ]
                            'provenance': ProvenanceMark(59357d99)
                        ]
                    } [
                        'signed': Signature
                    ]
                } [
                    "response_arid": ENCRYPTED [
                        'hasRecipient': SealedMessage
                    ]
                ]
                ❰"session"❱: ARID(b2c49e75)
                ❰"validUntil"❱: 2026-01-07
            ]
            'date': 2025-12-31
        ]
    "#}).trim();
    assert_actual_expected!(invite.to_request().unwrap().request().to_envelope().format(), expected_format);

    let gstp_envelope = invite.to_envelope().unwrap();

    #[rustfmt::skip]
    let expected_format = (indoc! {r#"
        ENCRYPTED [
            'hasRecipient': SealedMessage
            'hasRecipient': SealedMessage
            'hasRecipient': SealedMessage
        ]
    "#}).trim();
    assert_actual_expected!(gstp_envelope.format(), expected_format);

    let alice_invite = DkgInvitation::from_invite(
        gstp_envelope,
        date,
        &coordinator,
        &alice,
    )
    .unwrap();

    assert_eq!(alice_invite.xid(), alice.xid());
    assert_eq!(alice_invite.response_arid(), alice_response_arid);
    assert_eq!(alice_invite.valid_until(), expiry);
    assert_eq!(alice_invite.sender().xid(), coordinator.xid());
    assert_eq!(alice_invite.request_id(), request_id);
    assert!(alice_invite.peer_continuation().is_some());
    assert_eq!(alice_invite.min_signers(), min_signers);
    assert_eq!(alice_invite.charter(), charter);
    assert_eq!(alice_invite.session_id(), session_id);
}
