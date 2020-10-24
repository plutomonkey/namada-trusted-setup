use crate::{testing::prelude::*, Coordinator, Participant};
use phase1::{helpers::CurveKind, ContributionMode, ProvingSystem};

use std::panic;

fn create_contributor(id: &str) -> Participant {
    Participant::Contributor(format!("test-contributor-{}", id))
}

fn create_verifier(id: &str) -> Participant {
    Participant::Verifier(format!("test-verifier-{}", id))
}

fn execute_round_test(proving_system: ProvingSystem, curve: CurveKind) -> anyhow::Result<()> {
    let settings = (
        ContributionMode::Chunked,
        proving_system,
        curve,
        7,  /* power */
        32, /* batch_size */
        32, /* chunk_size */
    );
    let environment = initialize_test_environment_with_debug(
        &crate::environment::Testing::from(crate::environment::Parameters::Custom(settings)).into(),
    );
    let number_of_chunks = environment.number_of_chunks() as usize;

    // Instantiate a coordinator.
    let coordinator = Coordinator::new(environment)?;

    // Initialize the ceremony to round 1.
    assert_eq!(0, coordinator.current_round_height()?);
    coordinator.initialize()?;
    coordinator.update()?;
    assert_eq!(1, coordinator.current_round_height()?);

    // Add a contributor and verifier to the queue.
    let contributor = create_contributor("1");
    let verifier = create_verifier("1");
    coordinator.add_to_queue(contributor.clone(), 10)?;
    coordinator.add_to_queue(verifier.clone(), 10)?;
    assert_eq!(1, coordinator.number_of_queue_contributors());
    assert_eq!(1, coordinator.number_of_queue_verifiers());

    // Update the ceremony to round 2.
    coordinator.update()?;
    assert_eq!(2, coordinator.current_round_height()?);
    assert_eq!(0, coordinator.number_of_queue_contributors());
    assert_eq!(0, coordinator.number_of_queue_verifiers());

    // Contribute and verify up to the penultimate chunk.
    for _ in 0..number_of_chunks {
        coordinator.contribute(&contributor)?;
        coordinator.verify(&verifier)?;
    }

    //
    // Add a contributor and verifier to the queue.
    //
    // Note: This logic for adding to the queue works because
    // `Environment::allow_current_contributors_in_queue`
    // and `Environment::allow_current_verifiers_in_queue`
    // are set to `true`. This section can be removed without
    // changing the outcome of this test, if necessary.
    //
    let contributor = create_contributor("1");
    let verifier = create_verifier("1");
    coordinator.add_to_queue(contributor.clone(), 10)?;
    coordinator.add_to_queue(verifier.clone(), 10)?;
    assert_eq!(1, coordinator.number_of_queue_contributors());
    assert_eq!(1, coordinator.number_of_queue_verifiers());

    // Update the ceremony to round 3.
    coordinator.update()?;
    assert_eq!(3, coordinator.current_round_height()?);
    assert_eq!(0, coordinator.number_of_queue_contributors());
    assert_eq!(0, coordinator.number_of_queue_verifiers());

    Ok(())
}

fn coordinator_drop_contributor_test() -> anyhow::Result<()> {
    let environment = initialize_test_environment(&TEST_ENVIRONMENT_3);
    let number_of_chunks = environment.number_of_chunks() as usize;

    // Instantiate a coordinator.
    let coordinator = Coordinator::new(environment)?;

    // Initialize the ceremony to round 1.
    assert_eq!(0, coordinator.current_round_height()?);
    coordinator.initialize()?;
    coordinator.update()?;
    assert_eq!(1, coordinator.current_round_height()?);

    // Add a contributor and verifier to the queue.
    let contributor1 = create_contributor("1");
    let contributor2 = create_contributor("2");
    let verifier = create_verifier("1");
    coordinator.add_to_queue(contributor1.clone(), 10)?;
    coordinator.add_to_queue(contributor2.clone(), 9)?;
    coordinator.add_to_queue(verifier.clone(), 10)?;
    assert_eq!(2, coordinator.number_of_queue_contributors());
    assert_eq!(1, coordinator.number_of_queue_verifiers());
    assert!(coordinator.is_queue_contributor(&contributor1));
    assert!(coordinator.is_queue_contributor(&contributor2));
    assert!(coordinator.is_queue_verifier(&verifier));
    assert!(!coordinator.is_current_contributor(&contributor1));
    assert!(!coordinator.is_current_contributor(&contributor2));
    assert!(!coordinator.is_current_verifier(&verifier));
    assert!(!coordinator.is_finished_contributor(&contributor1));
    assert!(!coordinator.is_finished_contributor(&contributor2));
    assert!(!coordinator.is_finished_verifier(&verifier));

    // Update the ceremony to round 2.
    coordinator.update()?;
    assert_eq!(2, coordinator.current_round_height()?);
    assert_eq!(0, coordinator.number_of_queue_contributors());
    assert_eq!(0, coordinator.number_of_queue_verifiers());
    assert!(!coordinator.is_queue_contributor(&contributor1));
    assert!(!coordinator.is_queue_contributor(&contributor2));
    assert!(!coordinator.is_queue_verifier(&verifier));
    assert!(coordinator.is_current_contributor(&contributor1));
    assert!(coordinator.is_current_contributor(&contributor2));
    assert!(coordinator.is_current_verifier(&verifier));
    assert!(!coordinator.is_finished_contributor(&contributor1));
    assert!(!coordinator.is_finished_contributor(&contributor2));
    assert!(!coordinator.is_finished_verifier(&verifier));

    // Contribute and verify up to the penultimate chunk.
    for _ in 0..(number_of_chunks - 1) {
        coordinator.contribute(&contributor1)?;
        coordinator.contribute(&contributor2)?;
        coordinator.verify(&verifier)?;
        coordinator.verify(&verifier)?;
    }
    assert!(!coordinator.is_queue_contributor(&contributor1));
    assert!(!coordinator.is_queue_contributor(&contributor2));
    assert!(!coordinator.is_queue_verifier(&verifier));
    assert!(coordinator.is_current_contributor(&contributor1));
    assert!(coordinator.is_current_contributor(&contributor2));
    assert!(coordinator.is_current_verifier(&verifier));
    assert!(!coordinator.is_finished_contributor(&contributor1));
    assert!(!coordinator.is_finished_contributor(&contributor2));
    assert!(!coordinator.is_finished_verifier(&verifier));

    // Drop the contributor from the current round.
    let locators = coordinator.drop_participant(&contributor1)?;
    assert_eq!(&number_of_chunks - 1, locators.len());
    assert!(!coordinator.is_queue_contributor(&contributor1));
    assert!(!coordinator.is_queue_contributor(&contributor2));
    assert!(!coordinator.is_queue_verifier(&verifier));
    assert!(!coordinator.is_current_contributor(&contributor1));
    assert!(coordinator.is_current_contributor(&contributor2));
    assert!(coordinator.is_current_verifier(&verifier));
    assert!(!coordinator.is_finished_contributor(&contributor1));
    assert!(!coordinator.is_finished_contributor(&contributor2));
    assert!(!coordinator.is_finished_verifier(&verifier));

    // Fetch the coordinator state to begin inspection.
    let state = coordinator.state();
    let state = state.read().unwrap();
    assert_eq!(2, state.current_round_height());
    // assert_eq!(7, state.pending_verification.len());

    Ok(())
}

#[test]
#[serial]
fn test_round_on_groth16_bls12_377() {
    execute_round_test(ProvingSystem::Groth16, CurveKind::Bls12_377).unwrap();
}

#[test]
#[serial]
fn test_round_on_groth16_bw6_761() {
    execute_round_test(ProvingSystem::Groth16, CurveKind::BW6).unwrap();
}

#[test]
#[serial]
fn test_round_on_marlin_bls12_377() {
    execute_round_test(ProvingSystem::Marlin, CurveKind::Bls12_377).unwrap();
}

#[test]
#[serial]
#[ignore]
fn test_coordinator_drop_contributor() {
    coordinator_drop_contributor_test().unwrap();
}
