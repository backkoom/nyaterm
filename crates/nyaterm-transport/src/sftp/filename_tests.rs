//! A byte-oriented SFTP peer: string-based server handlers would hide path corruption.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use nyaterm_core::character_encoding::CharacterEncoding;
use russh_sftp::client::SftpSession;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::{OpenSftpConnection, OpenSftpSession, RemoteFilePath, SftpService};
use crate::session_config::{SftpSettings, SshSessionConfig};

#[derive(Default)]
struct Files {
    contents: HashMap<Vec<u8>, Vec<u8>>,
    directories: HashSet<Vec<u8>>,
    requests: usize,
}

fn number(input: &mut &[u8]) -> u32 {
    let (number, rest) = input.split_at(4);
    *input = rest;
    u32::from_be_bytes(number.try_into().unwrap())
}

fn string(input: &mut &[u8]) -> Vec<u8> {
    let length = number(input) as usize;
    let (value, rest) = input.split_at(length);
    *input = rest;
    value.to_vec()
}

fn push_string(output: &mut Vec<u8>, value: &[u8]) {
    output.extend_from_slice(&(value.len() as u32).to_be_bytes());
    output.extend_from_slice(value);
}

fn status(id: u32, code: u32) -> Vec<u8> {
    let mut packet = vec![101];
    packet.extend_from_slice(&id.to_be_bytes());
    packet.extend_from_slice(&code.to_be_bytes());
    push_string(&mut packet, b"");
    push_string(&mut packet, b"en");
    packet
}

fn attributes(packet: &mut Vec<u8>, size: usize, directory: bool) {
    packet.extend_from_slice(&5_u32.to_be_bytes());
    packet.extend_from_slice(&(size as u64).to_be_bytes());
    packet.extend_from_slice(&(if directory { 0o040755_u32 } else { 0o100644 }).to_be_bytes());
}

fn reply(
    input: &[u8],
    files: &mut Files,
    enumerated: &mut HashSet<Vec<u8>>,
    handles: &mut HashMap<Vec<u8>, Vec<u8>>,
) -> Vec<u8> {
    let kind = input[0];
    let mut input = &input[1..];
    let id = number(&mut input);
    if kind == 1 {
        let mut version = vec![2];
        version.extend_from_slice(&3_u32.to_be_bytes());
        return version;
    }
    files.requests += 1;
    let mut packet = Vec::new();
    match kind {
        3 | 11 => {
            let path = string(&mut input);
            if kind == 3 {
                let flags = number(&mut input);
                if flags & 8 != 0 {
                    files.contents.entry(path.clone()).or_default();
                }
                if !files.contents.contains_key(&path) {
                    return status(id, 2);
                }
            } else {
                enumerated.remove(&path);
            }
            packet.push(102);
            packet.extend_from_slice(&id.to_be_bytes());
            let handle = format!("h{}", handles.len()).into_bytes();
            handles.insert(handle.clone(), path);
            push_string(&mut packet, &handle);
        }
        4 | 9 => return status(id, 0),
        5 => {
            let handle = string(&mut input);
            let path = handles[&handle].clone();
            let offset = u64::from_be_bytes(input[..8].try_into().unwrap()) as usize;
            input = &input[8..];
            let length = number(&mut input) as usize;
            let content = &files.contents[&path];
            if offset >= content.len() {
                return status(id, 1);
            }
            packet.push(103);
            packet.extend_from_slice(&id.to_be_bytes());
            push_string(
                &mut packet,
                &content[offset..content.len().min(offset + length)],
            );
        }
        6 => {
            let handle = string(&mut input);
            let path = handles[&handle].clone();
            let offset = u64::from_be_bytes(input[..8].try_into().unwrap()) as usize;
            input = &input[8..];
            let bytes = string(&mut input);
            let content = files.contents.get_mut(&path).unwrap();
            content.resize(content.len().max(offset + bytes.len()), 0);
            content[offset..offset + bytes.len()].copy_from_slice(&bytes);
            return status(id, 0);
        }
        7 | 8 | 17 => {
            let path = string(&mut input);
            let path = if kind == 8 {
                handles[&path].clone()
            } else {
                path
            };
            let directory = files.directories.contains(&path);
            if !directory && !files.contents.contains_key(&path) {
                return status(id, 2);
            }
            packet.push(105);
            packet.extend_from_slice(&id.to_be_bytes());
            attributes(
                &mut packet,
                files.contents.get(&path).map_or(0, Vec::len),
                directory,
            );
        }
        12 | 16 => {
            let path = string(&mut input);
            let path = if kind == 12 {
                handles[&path].clone()
            } else {
                path
            };
            if kind == 12 && !enumerated.insert(path.clone()) {
                return status(id, 1);
            }
            let entries: Vec<(Vec<u8>, usize)> = if kind == 16 {
                vec![(path, 0)]
            } else {
                let mut prefix = path;
                if !prefix.ends_with(b"/") {
                    prefix.push(b'/');
                }
                files
                    .contents
                    .iter()
                    .filter_map(|(path, content)| {
                        path.strip_prefix(prefix.as_slice())
                            .filter(|name| !name.contains(&b'/'))
                            .map(|name| (name.to_vec(), content.len()))
                    })
                    .collect()
            };
            packet.push(104);
            packet.extend_from_slice(&id.to_be_bytes());
            packet.extend_from_slice(&(entries.len() as u32).to_be_bytes());
            for (name, size) in entries {
                push_string(&mut packet, &name);
                push_string(&mut packet, b"");
                attributes(&mut packet, size, kind == 16);
            }
        }
        13 => {
            let path = string(&mut input);
            return status(
                id,
                if files.contents.remove(&path).is_some() {
                    0
                } else {
                    2
                },
            );
        }
        14 => {
            files.directories.insert(string(&mut input));
            return status(id, 0);
        }
        18 => {
            let source = string(&mut input);
            let target = string(&mut input);
            let content = files.contents.remove(&source).unwrap();
            files.contents.insert(target, content);
            return status(id, 0);
        }
        _ => return status(id, 8),
    }
    packet
}

fn service(encoding: &str) -> (SftpService, Arc<Mutex<Files>>) {
    let service = SftpService::new(SshSessionConfig {
        encoding: encoding.to_string(),
        sftp: SftpSettings {
            compatibility_mode: true,
            ..Default::default()
        },
        ..Default::default()
    });
    let files = Arc::new(Mutex::new(Files::default()));
    files.lock().unwrap().directories.insert(b"/".to_vec());
    let peer_files = files.clone();
    let compatibility = service.compatibility.as_ref().unwrap();
    let session = compatibility
        .block_on(async move {
            let (client, mut server) = tokio::io::duplex(65536);
            tokio::spawn(async move {
                let mut enumerated = HashSet::new();
                let mut handles = HashMap::new();
                while let Ok(length) = server.read_u32().await {
                    assert!(length < 1024 * 1024);
                    let mut packet = vec![0; length as usize];
                    server.read_exact(&mut packet).await.unwrap();
                    let response = reply(
                        &packet,
                        &mut peer_files.lock().unwrap(),
                        &mut enumerated,
                        &mut handles,
                    );
                    server.write_u32(response.len() as u32).await.unwrap();
                    server.write_all(&response).await.unwrap();
                }
            });
            Ok(OpenSftpSession {
                sftp: Arc::new(SftpSession::new(client).await?),
                connection: Arc::new(Mutex::new(Some(OpenSftpConnection::Multiplex))),
                persistent: true,
            })
        })
        .unwrap();
    *compatibility.cache.lock().unwrap() = Some(session);
    (service, files)
}

#[test]
fn filename_codecs_preserve_paths_and_file_bytes_through_service_operations() {
    for (encoding, name) in [
        (CharacterEncoding::Utf8, "测试"),
        (CharacterEncoding::Gbk, "测试"),
        (CharacterEncoding::Gb18030, "测试😀"),
        (CharacterEncoding::Big5, "測試"),
        (CharacterEncoding::ShiftJis, "日本語"),
        (CharacterEncoding::EucKr, "한국어"),
    ] {
        let (service, files) = service(encoding.label());
        let directory = format!("/{name}");
        let original = format!("{directory}/{name}.txt");
        let renamed = format!("{directory}/{name}-2.txt");
        service.create_dir_path(&directory, None).unwrap();
        service.create_file_path(&original, None).unwrap();
        service.rename_path(&original, &renamed).unwrap();
        assert!(
            files
                .lock()
                .unwrap()
                .contents
                .contains_key(&encoding.encode(&renamed).unwrap())
        );
        let local = nyaterm_core::test_support::TestTempDir::new("nyaterm-sftp-encoded-path");
        std::fs::create_dir_all(&local).unwrap();
        let source = local.join("source");
        let download = local.join("download");
        let contents = b"raw\xff\x00\xb2\xe2";
        std::fs::write(&source, contents).unwrap();
        service.upload_file(&source, &renamed).unwrap();
        let entries = service.list_dir(&directory).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, format!("{name}-2.txt"));
        let path = entries[0].remote_path();
        assert_eq!(
            path.raw_path().unwrap().unwrap(),
            encoding.encode(&renamed).unwrap()
        );
        service
            .download_remote_file_with_progress_and_control_options(
                &path,
                &download,
                Default::default(),
                Default::default(),
                |_| {},
            )
            .unwrap();
        assert_eq!(std::fs::read(download).unwrap(), contents);
        service.delete_remote_path(&path).unwrap();
        assert!(files.lock().unwrap().contents.is_empty());
    }
}

#[test]
fn lossy_labels_keep_distinct_path_identity_and_invalid_new_names_send_no_requests() {
    let (service, files) = service("GBK");
    files.lock().unwrap().contents.extend([
        (b"/\xff".to_vec(), b"one".to_vec()),
        (b"/\xfe".to_vec(), b"two".to_vec()),
    ]);
    let entries = service.list_dir("/").unwrap();
    assert_eq!(entries[0].name, entries[1].name);
    let mut contents = Vec::new();
    for entry in entries {
        let path: RemoteFilePath = entry.remote_path();
        contents.push(
            service
                .read_file_bytes_path(&path, 128)
                .unwrap()
                .content_bytes,
        );
        service.delete_remote_path(&path).unwrap();
    }
    contents.sort();
    assert_eq!(contents, [b"one".to_vec(), b"two".to_vec()]);
    let before = files.lock().unwrap().requests;
    let error = service.create_file_path("/secret😀", None).unwrap_err();
    assert!(!format!("{error:?}").contains("secret"));
    assert_eq!(files.lock().unwrap().requests, before);
    assert!(service.create_dir_path("/secret😀", None).is_err());
    assert!(service.rename_path("/existing", "/secret😀").is_err());
    let local = nyaterm_core::test_support::TestTempDir::new("nyaterm-sftp-invalid-upload");
    std::fs::create_dir_all(&local).unwrap();
    let source = local.join("source");
    std::fs::write(&source, b"data").unwrap();
    assert!(service.upload_file(&source, "/secret😀").is_err());
    assert_eq!(files.lock().unwrap().requests, before);
}
