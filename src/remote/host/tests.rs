use crate::{
    command_bus::{
        AdminCommand, Command, CommandBus, CommandEnvelope, Conditions, Principal, ResourceRef,
    },
    host::{self, CommandState, DeviceRegistration},
    settings::{Error, SettingsClient},
    thread::{EnvironmentRef, HostId},
};

#[tokio::test]
async fn epoch_close_and_admission_share_the_writer_boundary() {
    let dir = tempfile::tempdir().unwrap();
    let client = SettingsClient::create(&dir.path().join("runtime.db"))
        .await
        .unwrap();
    let host = client.initialize_runtime().await.unwrap();
    let device = uuid::Uuid::new_v4().to_string();
    client
        .register_runtime_device(
            DeviceRegistration::new(
                device.clone(),
                "test device".into(),
                vec![1; 32],
                host::ALL_SCOPES,
                vec!["private".into()],
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let grant = client.lookup_runtime_device(vec![1; 32]).await.unwrap();
    let bus = CommandBus::new(client.clone());
    let mut epoch = 1;
    // Both callers are live concurrently; whichever the writer admits first
    // determines the only two valid outcomes. Neither outcome loses a command.
    for _ in 0..4 {
        let prepared = bus
            .prepare(
                CommandEnvelope {
                    command_id: uuid::Uuid::new_v4(),
                    resource: ResourceRef::Environment(EnvironmentRef {
                        host_id: HostId::new(host.id().into()).unwrap(),
                    }),
                    command: Command::Admin(AdminCommand::RevokeDevice {
                        device: uuid::Uuid::new_v4().to_string(),
                    }),
                    conditions: Conditions::default(),
                },
                Principal::authenticated(&grant, epoch).unwrap(),
            )
            .await
            .unwrap();
        let input = prepared.runtime_input().clone();
        let (close, admit) = tokio::join!(
            client.close_runtime_command_epoch(device.clone(), epoch),
            client.accept_runtime_command(input.clone(), host::ADMIN)
        );
        match (close, admit) {
            (Ok(next), Err(Error::CommandRetired)) => {
                assert_eq!(next, epoch + 1);
            }
            (Err(Error::OperationPending), Ok(receipt)) => {
                assert_eq!(receipt.state(), CommandState::Accepted);
                client
                    .transition_runtime_command(
                        receipt.key().clone(),
                        CommandState::Accepted,
                        CommandState::Rejected,
                        None,
                    )
                    .await
                    .unwrap();
                assert_eq!(
                    client
                        .close_runtime_command_epoch(device.clone(), epoch)
                        .await
                        .unwrap(),
                    epoch + 1
                );
            }
            outcome => panic!("Close/admission violated the transactional boundary: {outcome:?}"),
        }
        assert_eq!(
            client
                .close_runtime_command_epoch(device.clone(), epoch)
                .await
                .unwrap(),
            epoch + 1
        );
        assert!(matches!(
            client.accept_runtime_command(input, host::ADMIN).await,
            Err(Error::CommandRetired)
        ));
        epoch += 1;
    }
    assert!(matches!(
        client.close_runtime_command_epoch(device.clone(), 1).await,
        Err(Error::CommandRetired)
    ));
    client.revoke_runtime_device(device.clone()).await.unwrap();
    assert!(matches!(
        client.close_runtime_command_epoch(device, epoch).await,
        Err(Error::NotAuthorized)
    ));
    client.shutdown().await.unwrap();
}
