use super::proto as p;
use prost::Message;
use std::collections::BTreeMap;

// prost-types intentionally discards unknown extensions. These minimal descriptor
// projections decode the actual encoded MethodOptions, including our extension,
// with prost's normal decoder rather than source matching or a second parser.
#[derive(Clone, PartialEq, Message)]
struct DescriptorSet {
    #[prost(message, repeated, tag = "1")]
    files: Vec<File>,
}
#[derive(Clone, PartialEq, Message)]
struct File {
    #[prost(string, optional, tag = "2")]
    package: Option<String>,
    #[prost(message, repeated, tag = "6")]
    services: Vec<Service>,
}
#[derive(Clone, PartialEq, Message)]
struct Service {
    #[prost(message, repeated, tag = "2")]
    methods: Vec<Method>,
}
#[derive(Clone, PartialEq, Message)]
struct Method {
    #[prost(string, optional, tag = "1")]
    name: Option<String>,
    #[prost(string, optional, tag = "3")]
    output: Option<String>,
    #[prost(message, optional, tag = "4")]
    options: Option<Options>,
}
#[derive(Clone, PartialEq, Message)]
struct Options {
    #[prost(message, optional, tag = "51000")]
    command: Option<p::CommandOption>,
}

#[test]
fn encoded_commands_match_the_registry_exactly() {
    let descriptors = DescriptorSet::decode(p::DESCRIPTOR).unwrap();
    let mut commands = BTreeMap::new();
    let mut reads = Vec::new();
    for file in descriptors
        .files
        .into_iter()
        .filter(|f| f.package.as_deref() == Some("canopy.api.v1"))
    {
        for method in file.services.into_iter().flat_map(|s| s.methods) {
            let name = method.name.unwrap();
            assert_ne!(name.to_lowercase(), "execute");
            let option = method.options.and_then(|o| o.command);
            if method.output.as_deref() == Some(".canopy.api.v1.CommandReceipt") {
                let option = option.unwrap_or_else(|| panic!("{name} is missing command metadata"));
                assert!(
                    commands.insert(option.name.clone(), option).is_none(),
                    "duplicate command RPC: {name}"
                );
            } else {
                assert!(option.is_none(), "read {name} declares a command");
                reads.push(name);
            }
        }
    }
    assert_eq!(
        reads,
        [
            "ListThreads",
            "ReadCatalog",
            "ListResources",
            "WatchSummaries",
            "ListAgents",
            "GetAgentCatalog",
            "ListPullRequests",
            "ReadThreadPage",
            "PreflightClose",
            "PreflightCloseProject",
            "PreflightCloseTerminal",
            "SubscribeThread",
            "ReadHistoryPage",
            "SearchHistory",
            "GetAgentOptions",
            "PreviewEdit",
            "GetSettings",
            "GetInfo",
            "WatchHost",
            "ListDevices",
            "ReadDiagnostics",
            "GetCommand",
            "ResolveCommand",
            "CloseCommandEpoch",
            "GetPeerRequest",
            "RequestPairing",
            "GetPairingStatus",
            "GetUpload",
            "PutChunk",
            "Download",
            "ReadHistory",
            "TerminalSession",
            "ListDirectory",
            "SearchFiles",
            "ReadFile",
            "Preflight",
            "GetStatus",
            "ReadDiff",
            "ReadHistory",
            "ListRefs",
            "ListIntegrations",
            "ListProjects",
            "QueryTasks",
            "GetTask",
            "GetSchema",
            "ReadOptions",
            "ReadComments",
            "ReadAttachment"
        ]
    );
    for entry in crate::command_bus::registry() {
        let option = commands
            .remove(entry.name)
            .unwrap_or_else(|| panic!("{} has no RPC", entry.name));
        let mut scope = 0;
        for value in option.scopes {
            let bit = match p::Scope::try_from(value).unwrap() {
                p::Scope::Read => crate::host::READ,
                p::Scope::Chat => crate::host::CHAT,
                p::Scope::Approve => crate::host::APPROVE,
                p::Scope::FilesGit => crate::host::FILES_GIT,
                p::Scope::Terminal => crate::host::TERMINAL,
                p::Scope::Tasks => crate::host::TASKS,
                p::Scope::Admin => crate::host::ADMIN,
                p::Scope::Unspecified => panic!("{} unspecified scope", entry.name),
            };
            assert_eq!(scope & bit, 0, "{} duplicate scope", entry.name);
            scope |= bit;
        }
        assert_eq!(scope, entry.scope, "{} scope", entry.name);
        let risk = match entry.risk {
            crate::command_bus::Risk::Metadata => p::Risk::Metadata,
            crate::command_bus::Risk::Delivery => p::Risk::Delivery,
            crate::command_bus::Risk::Configuration => p::Risk::Configuration,
        };
        assert_eq!(option.risk, risk as i32, "{} risk", entry.name);
    }
    assert!(
        commands.is_empty(),
        "RPC absent from the registry: {commands:?}"
    );
    // TODO: compare the real tool registry here when that adapter is implemented.
    // No empty or hard-coded tool list may stand in for that parity check.
}

#[test]
fn inbox_maintenance_has_only_the_two_dedicated_protocol_methods() {
    let descriptors = prost_types::FileDescriptorSet::decode(p::DESCRIPTOR).unwrap();
    let file = descriptors
        .file
        .iter()
        .find(|f| f.package.as_deref() == Some("canopy.api.v1"))
        .unwrap();
    let host = file
        .service
        .iter()
        .find(|s| s.name.as_deref() == Some("HostService"))
        .unwrap();
    for (name, input, output) in [
        (
            "CloseCommandEpoch",
            "CloseCommandEpochRequest",
            "CommandEpoch",
        ),
        (
            "ResolveCommand",
            "ResolveCommandRequest",
            "ResolveCommandResponse",
        ),
    ] {
        let method = host
            .method
            .iter()
            .find(|m| m.name.as_deref() == Some(name))
            .unwrap();
        assert_eq!(
            method.input_type.as_deref(),
            Some(format!(".canopy.api.v1.{input}").as_str())
        );
        assert_eq!(
            method.output_type.as_deref(),
            Some(format!(".canopy.api.v1.{output}").as_str())
        );
        let request = file
            .message_type
            .iter()
            .find(|m| m.name.as_deref() == Some(input))
            .unwrap();
        assert!(
            request
                .field
                .iter()
                .all(|f| f.type_name.as_deref() != Some(".canopy.api.v1.CommandMeta")),
            "{name} must not admit another inbox command"
        );
    }
    let close = file
        .message_type
        .iter()
        .find(|m| m.name.as_deref() == Some("CloseCommandEpochRequest"))
        .unwrap();
    assert_eq!(
        close
            .field
            .iter()
            .map(|f| f.name.as_deref().unwrap())
            .collect::<Vec<_>>(),
        ["environment", "expected_epoch"]
    );
    let resolve = file
        .message_type
        .iter()
        .find(|m| m.name.as_deref() == Some("ResolveCommandResponse"))
        .unwrap();
    assert_eq!(
        resolve.field[0].type_name.as_deref(),
        Some(".canopy.api.v1.CommandReceipt")
    );
}

#[test]
fn public_contract_is_standalone_and_thread_commands_have_no_parallel_project() {
    let descriptors = prost_types::FileDescriptorSet::decode(p::DESCRIPTOR).unwrap();
    let file = descriptors
        .file
        .iter()
        .find(|f| f.package.as_deref() == Some("canopy.api.v1"))
        .unwrap();
    assert_eq!(
        file.dependency,
        [
            "google/protobuf/descriptor.proto",
            "google/protobuf/struct.proto"
        ]
    );
    let terminal = file
        .service
        .iter()
        .find(|service| service.name.as_deref() == Some("TerminalService"))
        .unwrap();
    for method in &terminal.method {
        if method.output_type.as_deref() != Some(".canopy.api.v1.CommandReceipt") {
            continue;
        }
        let request = file
            .message_type
            .iter()
            .find(|message| {
                method.input_type.as_deref()
                    == Some(format!(".canopy.api.v1.{}", message.name.as_deref().unwrap()).as_str())
            })
            .unwrap();
        assert!(
            request
                .field
                .iter()
                .any(|field| field.type_name.as_deref() == Some(".canopy.api.v1.ProjectRef")),
            "Terminal lifecycle requires a project identity"
        );
        assert!(
            request.field.iter().all(|field| field.type_name.as_deref()
                != Some(".canopy.api.v1.EnvironmentRef")
                && !["path", "cwd"].contains(&field.name.as_deref().unwrap())),
            "Private/scratch identity must remain local to the bus"
        );
    }
    for message in &file.message_type {
        assert!(!["ChatRef", "WorkspaceRef"].contains(&message.name.as_deref().unwrap()));
        for field in &message.field {
            assert!(
                !field
                    .type_name
                    .as_deref()
                    .unwrap_or_default()
                    .contains("canopy.remote")
            );
            if message
                .name
                .as_deref()
                .is_some_and(|n| n.ends_with("Request"))
                && message
                    .field
                    .iter()
                    .any(|f| f.type_name.as_deref() == Some(".canopy.api.v1.ThreadTarget"))
            {
                assert!(
                    !["project", "worktree", "workspace"].iter().any(|n| field
                        .name
                        .as_deref()
                        .unwrap()
                        .contains(n))
                );
            }
        }
    }
}

#[test]
fn project_reference_rejects_wrong_checkout_using_authoritative_metadata() {
    use crate::state::projects::Projects;
    use crate::thread::HostId;
    let host = HostId::new(uuid::Uuid::new_v4().to_string()).unwrap();
    let mut projects = Projects::default();
    // These paths deliberately do not exist: validation must use owner metadata,
    // without probing a repository or resolving any user-supplied filesystem path.
    projects.open_worktree("/unopened/repository".into(), "/unopened/checkout".into());
    let record = projects.current().unwrap();
    let valid = p::ProjectRef {
        host_id: host.to_string(),
        project_id: serde_json::to_string(&record.id).unwrap(),
        worktree_id: Some(serde_json::to_string(&record.workspace).unwrap()),
    };
    assert!(super::checked_project_ref(valid.clone(), &host, record).is_ok());
    let mut missing = valid.clone();
    missing.worktree_id = None;
    assert_eq!(
        super::checked_project_ref(missing, &host, record)
            .unwrap_err()
            .code(),
        tonic::Code::InvalidArgument
    );
    let mut foreign = valid.clone();
    foreign.worktree_id = Some("other-checkout".into());
    assert_eq!(
        super::checked_project_ref(foreign, &host, record)
            .unwrap_err()
            .code(),
        tonic::Code::NotFound
    );
    let mut wrong_host = valid.clone();
    wrong_host.host_id = uuid::Uuid::new_v4().to_string();
    assert_eq!(
        super::checked_project_ref(wrong_host, &host, record)
            .unwrap_err()
            .code(),
        tonic::Code::NotFound
    );
    let mut ordinary = record.clone();
    ordinary.repository_path = None;
    assert_eq!(
        super::checked_project_ref(valid.clone(), &host, &ordinary)
            .unwrap_err()
            .code(),
        tonic::Code::InvalidArgument
    );
    let mut valid_ordinary = valid;
    valid_ordinary.worktree_id = None;
    assert!(super::checked_project_ref(valid_ordinary, &host, &ordinary).is_ok());
}
