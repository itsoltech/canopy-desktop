-- One unreleased migration for agent runtime and remote. Edit this file until
-- publication; do not add development-stage migrations.
CREATE TABLE _canopy_runtime_meta (
    id INTEGER PRIMARY KEY CHECK(id=1),
    version INTEGER NOT NULL,
    publication TEXT NOT NULL,
    fingerprint TEXT NOT NULL
);
CREATE TABLE _canopy_runtime_host (
    id INTEGER PRIMARY KEY CHECK(id=1),
    host_id TEXT NOT NULL UNIQUE,
    id_floors TEXT NOT NULL DEFAULT '{}',
    tls_identity_ref TEXT,
    presence_revision INTEGER NOT NULL DEFAULT 0 CHECK(presence_revision>=0)
);
CREATE TABLE _canopy_runtime_chats (
    chat_id TEXT PRIMARY KEY,
    workspace TEXT,
    authority_root TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    profile_id TEXT,
    metadata TEXT NOT NULL,
    recovery_reason TEXT,
    history_epoch INTEGER NOT NULL DEFAULT 0 CHECK(history_epoch>=0),
    checkpoint INTEGER NOT NULL DEFAULT 0 CHECK(checkpoint>=0),
    queue_revision INTEGER NOT NULL DEFAULT 0 CHECK(queue_revision>=0),
    queue_paused INTEGER NOT NULL DEFAULT 1 CHECK(queue_paused IN (0,1)),
    closed_at INTEGER
);
CREATE TABLE _canopy_remote_devices (
    device_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    public_credential BLOB NOT NULL,
    scopes INTEGER NOT NULL,
    roots TEXT NOT NULL,
    authorization_revision INTEGER NOT NULL DEFAULT 1,
    command_epoch INTEGER NOT NULL DEFAULT 1 CHECK(command_epoch>0),
    revoked_at INTEGER
);
CREATE UNIQUE INDEX _canopy_device_credential ON _canopy_remote_devices(public_credential);
CREATE TABLE _canopy_command_inbox (
    device_id TEXT NOT NULL,
    command_id TEXT NOT NULL,
    command_epoch INTEGER NOT NULL,
    method TEXT NOT NULL,
    target TEXT NOT NULL,
    fingerprint BLOB NOT NULL,
    required_scope INTEGER NOT NULL,
    required_root TEXT NOT NULL,
    arguments BLOB,
    state TEXT NOT NULL CHECK(state IN ('accepted','attempting','succeeded','failed','rejected','delivery_unknown')),
    result BLOB,
    resolved_by_user INTEGER NOT NULL DEFAULT 0 CHECK(resolved_by_user IN (0,1)),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY(device_id,command_id)
);
CREATE INDEX _canopy_command_device_state ON _canopy_command_inbox(device_id,state);
CREATE TABLE _canopy_chat_queue (
    chat_id TEXT NOT NULL REFERENCES _canopy_runtime_chats(chat_id) ON DELETE CASCADE,
    message_id TEXT NOT NULL,
    position INTEGER NOT NULL CHECK(position>=0),
    payload TEXT NOT NULL,
    state TEXT NOT NULL CHECK(state IN ('queued','attempting','delivered','failed','dismissed','delivery_unknown')),
    source_device TEXT NOT NULL,
    steer_run INTEGER CHECK(steer_run IS NULL OR steer_run>=0),
    resolution TEXT,
    resolved_by_user INTEGER NOT NULL DEFAULT 0 CHECK(resolved_by_user IN (0,1)),
    PRIMARY KEY(chat_id,message_id)
);
CREATE INDEX _canopy_queue_order ON _canopy_chat_queue(chat_id,position);
CREATE TABLE _canopy_dirty_presence (
    device_id TEXT NOT NULL,
    resource TEXT NOT NULL,
    base_hash BLOB NOT NULL,
    last_seen INTEGER NOT NULL,
    revision INTEGER NOT NULL CHECK(revision>0),
    PRIMARY KEY(device_id,resource)
);
CREATE INDEX _canopy_dirty_resource ON _canopy_dirty_presence(resource,device_id);
CREATE TABLE _canopy_remote_hosts (
    host_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    pinned_identity BLOB NOT NULL,
    credential_ref TEXT NOT NULL,
    routes TEXT NOT NULL,
    last_seen INTEGER
);
CREATE TABLE _canopy_remote_outbox (
    entry_id TEXT PRIMARY KEY,
    host_id TEXT NOT NULL,
    chat_id TEXT NOT NULL,
    profile_id TEXT,
    context TEXT NOT NULL,
    payload TEXT NOT NULL,
    command_epoch INTEGER,
    state TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE TABLE _canopy_artifacts (
    artifact_id TEXT PRIMARY KEY,
    content_hash BLOB NOT NULL,
    size INTEGER NOT NULL CHECK(size>=0),
    media_type TEXT NOT NULL,
    relative_path TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE TABLE _canopy_artifact_refs (
    artifact_id TEXT NOT NULL REFERENCES _canopy_artifacts(artifact_id) ON DELETE CASCADE,
    owner_kind TEXT NOT NULL,
    owner_id TEXT NOT NULL,
    PRIMARY KEY(artifact_id,owner_kind,owner_id)
);
CREATE TABLE _canopy_uploads (
    upload_id TEXT PRIMARY KEY,
    device_id TEXT NOT NULL,
    target TEXT NOT NULL,
    purpose TEXT NOT NULL,
    expected_size INTEGER NOT NULL CHECK(expected_size>=0),
    expected_hash BLOB NOT NULL,
    media_type TEXT NOT NULL,
    ranges TEXT NOT NULL,
    artifact_id TEXT,
    last_active INTEGER NOT NULL
);

-- Delete invalidates publication and releases only that chat's references.
CREATE TRIGGER _canopy_artifact_chat_delete AFTER DELETE ON _canopy_runtime_chats BEGIN
    DELETE FROM _canopy_uploads WHERE json_extract(target,'$.Chat')=OLD.chat_id AND artifact_id IS NULL;
    DELETE FROM _canopy_artifact_refs WHERE owner_kind='chat' AND owner_id=OLD.chat_id;
END;
CREATE TRIGGER _canopy_artifact_device_revoke AFTER UPDATE OF revoked_at ON _canopy_remote_devices WHEN NEW.revoked_at IS NOT NULL BEGIN
    DELETE FROM _canopy_uploads WHERE device_id=NEW.device_id AND artifact_id IS NULL;
END;

CREATE TRIGGER _canopy_artifact_command_delete AFTER DELETE ON _canopy_command_inbox BEGIN
    DELETE FROM _canopy_artifact_refs WHERE owner_kind='command' AND owner_id=json_array(OLD.device_id,OLD.command_id);
END;
CREATE TABLE IF NOT EXISTS _canopy_settings_secrets (
    version_id TEXT PRIMARY KEY,
    device_id TEXT NOT NULL,
    command_id TEXT NOT NULL,
    command_epoch INTEGER NOT NULL,
    fingerprint BLOB NOT NULL,
    created_at INTEGER NOT NULL
);
