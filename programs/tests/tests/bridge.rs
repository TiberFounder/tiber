//! The bridge: SPL tokens and SOL in and out of the token standard.

use tiber_bridge::client as bridge;
use tiber_bridge::constants::NATIVE_MINT;
use tiber_bridge::state::Wrapper;
use tiber_program_tests::fixture::World;
use solana_signer::Signer;

#[test]
fn sol_round_trip() {
    let mut w = World::new();
    let user = w.env.funded(5_000_000_000);
    let wrapped = bridge::wrapped_mint_address(&NATIVE_MINT);
    assert_eq!(w.sol, wrapped);
    let before = w.env.lamports(&user.pubkey());
    let tx = w.wrap_sol(&user, 2_000_000_000);
    println!("wrap_sol CU {} size {}", tx.cu(), tx.size);
    tx.ok();
    assert_eq!(w.env.holding(&wrapped, &user.pubkey()), 2_000_000_000);
    let spent = before - w.env.lamports(&user.pubkey());
    assert!(
        (2_000_000_000..2_010_000_000).contains(&spent),
        "spent {spent}"
    );
    assert_eq!(
        w.env
            .read::<Wrapper>(&bridge::wrapper_address(&NATIVE_MINT))
            .total_wrapped,
        2_000_000_000
    );

    let before = w.env.lamports(&user.pubkey());
    let tx = w
        .env
        .send_paid_by(&[bridge::unwrap_sol(user.pubkey(), u64::MAX)], &user, &[]);
    tx.ok();
    assert_eq!(w.env.holding(&wrapped, &user.pubkey()), 0);
    let got = w.env.lamports(&user.pubkey()) - before;
    assert!((1_990_000_001..=2_000_000_000).contains(&got), "got {got}");
    assert_eq!(
        w.env
            .read::<Wrapper>(&bridge::wrapper_address(&NATIVE_MINT))
            .total_wrapped,
        0
    );
    // Nothing left to unwrap.
    w.env
        .send_paid_by(&[bridge::unwrap_sol(user.pubkey(), u64::MAX)], &user, &[])
        .expect_fail();
}
