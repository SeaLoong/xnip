//! Bounded release delivery. Scheduling, leases and the singleton belong to the caller.

use std::fs::{self, File};
use std::io::{self, Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};
use flate2::read::GzDecoder;
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tempfile::{Builder, NamedTempFile};

pub const DEFAULT_RELEASE_URL: &str = "https://api.github.com/repos/SeaLoong/xnip/releases/latest";
pub const DEFAULT_PROXY: &str = "https://gh-proxy.org";
const DOWNLOAD_ROOT: &str = "https://github.com/SeaLoong/xnip/releases/download/";
const MAX_METADATA: u64 = 1024 * 1024;
const MAX_CHECKSUM: u64 = 4096;
const MAX_ARCHIVE: u64 = 64 * 1024 * 1024;
const MAX_BINARY: u64 = 128 * 1024 * 1024;
const MAX_EXPANDED: u64 = 160 * 1024 * 1024;
const MAX_ENTRIES: usize = 32;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(45);
const PREPARE_TIMEOUT: Duration = Duration::from_secs(150);

#[derive(Clone, Debug)]
pub struct DeliveryConfig {
    pub release_url: String,
    /// HTTPS origin prefixed to official URLs; None requests GitHub directly.
    pub proxy: Option<String>,
    pub target: String,
    pub current_version: String,
}

impl Default for DeliveryConfig {
    fn default() -> Self {
        Self {
            release_url: DEFAULT_RELEASE_URL.to_owned(),
            proxy: Some(DEFAULT_PROXY.to_owned()),
            target: release_target(env!("XNIP_TARGET")).to_owned(),
            current_version: crate::VERSION.to_owned(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Prepared {
    pub version: String,
    pub path: PathBuf,
    /// Hash of the extracted executable, checked again immediately before installing.
    pub sha256: String,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    size: u64,
    #[serde(default)]
    digest: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct DeliveryTrace {
    pub route: Option<String>,
    pub proxy_error: Option<String>,
}

pub fn normalize_proxy(value: &str) -> Result<Option<String>> {
    if matches!(value, "0" | "direct") {
        return Ok(None);
    }
    let value = value.trim_end_matches('/');
    let uri: ureq::http::Uri = value.parse().context("invalid update proxy URL")?;
    ensure!(
        uri.scheme_str() == Some("https")
            && uri
                .authority()
                .is_some_and(|authority| !authority.as_str().contains('@'))
            && matches!(uri.path(), "" | "/"),
        "update proxy must be an HTTPS origin without credentials or a path"
    );
    ensure!(uri.query().is_none(), "update proxy cannot contain a query");
    ensure!(
        !value.contains('#'),
        "update proxy cannot contain a fragment"
    );
    Ok(Some(value.to_owned()))
}

struct Fetch {
    agent: ureq::Agent,
    fixture_origin: Option<String>,
    proxy: Option<String>,
    deadline: Instant,
}

impl Fetch {
    fn new(release_url: &str) -> Result<Self> {
        let fixture_origin = if release_url == DEFAULT_RELEASE_URL {
            None
        } else {
            // Deliberately not an environment override. Debug builds can receive an explicit
            // loopback endpoint from fixture-driven integration tests.
            ensure!(
                cfg!(any(debug_assertions, test)),
                "release endpoint is not the official endpoint"
            );
            Some(loopback_origin(release_url).context("release endpoint is not loopback")?)
        };
        // Official asset URLs redirect to GitHub CDN hosts. Permit those HTTPS
        // redirects; https_only also rejects a downgrade on any redirected request.
        // Explicit loopback fixtures never follow redirects.
        let agent = ureq::Agent::config_builder()
            .https_only(fixture_origin.is_none())
            .max_redirects(if fixture_origin.is_none() { 5 } else { 0 })
            .timeout_global(Some(REQUEST_TIMEOUT))
            .timeout_connect(Some(Duration::from_secs(10)))
            .max_response_header_size(32 * 1024)
            .user_agent(concat!("xnip/", env!("CARGO_PKG_VERSION")))
            .build()
            .into();
        Ok(Self {
            agent,
            fixture_origin,
            proxy: None,
            deadline: Instant::now() + PREPARE_TIMEOUT,
        })
    }

    fn check_asset_url(&self, url: &str, tag: &str, name: &str) -> Result<()> {
        let expected = self.fixture_origin.as_ref().map_or_else(
            || format!("{DOWNLOAD_ROOT}{tag}/{name}"),
            |origin| format!("{origin}/{name}"),
        );
        ensure!(
            url == expected,
            "release asset URL does not match its name and tag"
        );
        Ok(())
    }

    fn get(&self, url: &str, limit: u64) -> Result<Vec<u8>> {
        ensure!(
            Instant::now() < self.deadline,
            "release delivery deadline exceeded"
        );
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        let url = self
            .proxy
            .as_ref()
            .map_or_else(|| url.to_owned(), |proxy| format!("{proxy}/{url}"));
        let mut response = self
            .agent
            .get(&url)
            .config()
            .timeout_global(Some(remaining.min(REQUEST_TIMEOUT)))
            .build()
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .call()
            .context("release download failed")?;
        ensure!(
            response.status().is_success(),
            "release download did not succeed"
        );
        if let Some(length) = response.headers().get("content-length") {
            let length: u64 = length.to_str()?.parse()?;
            ensure!(length <= limit, "release download exceeds its size limit");
        }
        let mut bytes = Vec::new();
        response
            .body_mut()
            .as_reader()
            .take(limit + 1)
            .read_to_end(&mut bytes)
            .context("release body read failed")?;
        ensure!(
            bytes.len() as u64 <= limit,
            "release body exceeds its size limit"
        );
        ensure!(
            Instant::now() < self.deadline,
            "release delivery deadline exceeded"
        );
        Ok(bytes)
    }
}

fn loopback_origin(url: &str) -> Option<String> {
    let authority = url.strip_prefix("http://")?.split('/').next()?;
    let (host, port) = authority.rsplit_once(':')?;
    if !matches!(host, "127.0.0.1" | "localhost" | "[::1]") || port.parse::<u16>().ok()? == 0 {
        return None;
    }
    Some(format!("http://{authority}"))
}

// Static musl releases also run on GNU Linux installations built from source.
fn release_target(target: &str) -> &str {
    match target {
        "x86_64-unknown-linux-gnu" => "x86_64-unknown-linux-musl",
        "aarch64-unknown-linux-gnu" => "aarch64-unknown-linux-musl",
        _ => target,
    }
}

fn target_binary(target: &str) -> Result<&'static str> {
    match target {
        "x86_64-pc-windows-msvc" | "aarch64-pc-windows-msvc" => Ok("xnip.exe"),
        "x86_64-unknown-linux-musl"
        | "aarch64-unknown-linux-musl"
        | "x86_64-apple-darwin"
        | "aarch64-apple-darwin" => Ok("xnip"),
        _ => bail!("no published binary for this target"),
    }
}

fn one_asset<'a>(release: &'a Release, name: &str) -> Result<&'a Asset> {
    let mut matches = release.assets.iter().filter(|asset| asset.name == name);
    let asset = matches.next().context("release asset missing")?;
    ensure!(
        matches.next().is_none(),
        "release contains duplicate assets"
    );
    Ok(asset)
}

fn checksum(bytes: &[u8], asset_name: &str) -> Result<String> {
    let text = std::str::from_utf8(bytes).context("checksum is not UTF-8")?;
    let fields: Vec<_> = text.split_whitespace().collect();
    ensure!(fields.len() == 2, "checksum file must identify one archive");
    ensure!(
        fields[0].len() == 64 && fields[0].bytes().all(|byte| byte.is_ascii_hexdigit()),
        "invalid SHA256"
    );
    ensure!(
        fields[1].strip_prefix('*').unwrap_or(fields[1]) == asset_name,
        "checksum names a different archive"
    );
    Ok(fields[0].to_ascii_lowercase())
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Download and validate a stable release without running any downloaded program.
pub fn prepare(config: &DeliveryConfig, directory: &Path) -> Result<Option<Prepared>> {
    prepare_traced(config, directory, &mut DeliveryTrace::default())
}

/// Retain route failures privately even when direct fallback succeeds.
pub(crate) fn prepare_traced(
    config: &DeliveryConfig,
    directory: &Path,
    trace: &mut DeliveryTrace,
) -> Result<Option<Prepared>> {
    target_binary(&config.target)?;
    let mut fetch = Fetch::new(&config.release_url)?;
    if let Some(proxy) = &config.proxy {
        // Loopback proxy origins exist only in offline unit fixtures.
        fetch.proxy = if cfg!(test) && loopback_origin(proxy).as_deref() == Some(proxy) {
            Some(proxy.clone())
        } else {
            normalize_proxy(proxy)?
        };
    }
    if fetch.proxy.is_some() {
        trace.route = Some("proxy".into());
        match prepare_from(config, directory, &fetch) {
            Ok(prepared) => return Ok(prepared),
            Err(error) => {
                trace.proxy_error = Some(format!("{error:#}").chars().take(512).collect());
                fetch.proxy = None;
            }
        }
    }
    trace.route = Some("direct".into());
    prepare_from(config, directory, &fetch)
}

fn prepare_from(
    config: &DeliveryConfig,
    directory: &Path,
    fetch: &Fetch,
) -> Result<Option<Prepared>> {
    let binary = target_binary(&config.target)?;
    let metadata = fetch.get(&config.release_url, MAX_METADATA)?;
    let release: Release = serde_json::from_slice(&metadata).context("invalid release metadata")?;
    ensure!(
        !release.draft && !release.prerelease,
        "release is not stable"
    );
    let tag = &release.tag_name;
    let version = Version::parse(
        tag.strip_prefix('v')
            .context("release tag must start with v")?,
    )?;
    ensure!(version.pre.is_empty(), "release tag is a prerelease");
    let current = Version::parse(&config.current_version)?;
    if !version.cmp_precedence(&current).is_gt() {
        return Ok(None);
    }
    let extension = if binary == "xnip.exe" {
        "zip"
    } else {
        "tar.gz"
    };
    let name = format!("xnip-{tag}-{}.{extension}", config.target);
    let archive_asset = one_asset(&release, &name)?;
    let checksum_asset = one_asset(&release, &format!("{name}.sha256"))?;
    ensure!(
        archive_asset.size > 0 && archive_asset.size <= MAX_ARCHIVE,
        "invalid archive size"
    );
    ensure!(
        checksum_asset.size > 0 && checksum_asset.size <= MAX_CHECKSUM,
        "invalid checksum size"
    );
    fetch.check_asset_url(&archive_asset.browser_download_url, tag, &name)?;
    fetch.check_asset_url(
        &checksum_asset.browser_download_url,
        tag,
        &checksum_asset.name,
    )?;
    let expected = checksum(
        &fetch.get(&checksum_asset.browser_download_url, MAX_CHECKSUM)?,
        &name,
    )?;
    if let Some(digest) = &archive_asset.digest {
        ensure!(
            digest == &format!("sha256:{expected}"),
            "release digest disagrees with checksum"
        );
    }
    let archive = fetch.get(&archive_asset.browser_download_url, MAX_ARCHIVE)?;
    ensure!(
        archive.len() as u64 == archive_asset.size,
        "archive size disagrees with metadata"
    );
    ensure!(
        sha256(&archive) == expected,
        "release archive SHA256 mismatch"
    );

    fs::create_dir_all(directory).context("cannot create release staging directory")?;
    let mut staged = Builder::new()
        .prefix("candidate-")
        .suffix(if binary == "xnip.exe" { ".exe" } else { "" })
        .tempfile_in(directory)?;
    if binary == "xnip.exe" {
        extract_zip(&archive, binary, staged.as_file_mut())?;
    } else {
        extract_tar(&archive, binary, staged.as_file_mut())?;
    }
    staged.flush()?;
    staged.as_file().sync_all()?;
    let hash = hash_file(staged.path())?;
    let (_, path) = staged.keep()?;
    Ok(Some(Prepared {
        version: version.to_string(),
        path,
        sha256: hash,
    }))
}

fn safe_member(name: &[u8]) -> Result<String> {
    let text = std::str::from_utf8(name).context("archive path is not UTF-8")?;
    let text = text.strip_prefix("./").unwrap_or(text);
    ensure!(
        !text.is_empty()
            && !text.starts_with('/')
            && !text.contains(['\\', ':', '\0'])
            && text
                .split('/')
                .all(|part| !part.is_empty() && part != "." && part != ".."),
        "archive contains an unsafe path"
    );
    Ok(text.to_owned())
}

fn copy_binary(reader: &mut impl Read, destination: &mut File, size: u64) -> Result<()> {
    ensure!(size > 0 && size <= MAX_BINARY, "invalid executable size");
    let copied = io::copy(&mut reader.take(MAX_BINARY + 1), destination)?;
    ensure!(copied == size, "executable size disagrees with archive");
    Ok(())
}

fn extract_zip(bytes: &[u8], binary: &str, destination: &mut File) -> Result<()> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
    ensure!(archive.len() <= MAX_ENTRIES, "archive has too many members");
    let mut expanded = 0_u64;
    let mut found = false;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let name = safe_member(entry.name_raw())?;
        ensure!(
            entry
                .unix_mode()
                .is_none_or(|mode| mode & 0o170_000 == 0 || mode & 0o170_000 == 0o100_000),
            "archive member is not a regular file"
        );
        ensure!(!entry.is_dir(), "archive contains a directory");
        expanded = expanded
            .checked_add(entry.size())
            .context("expanded archive size overflow")?;
        ensure!(
            expanded <= MAX_EXPANDED,
            "expanded archive exceeds its size limit"
        );
        if name == binary {
            ensure!(!found, "archive contains duplicate executables");
            let size = entry.size();
            copy_binary(&mut entry, destination, size)?;
            found = true;
        }
    }
    ensure!(found, "archive executable missing");
    Ok(())
}

fn extract_tar(bytes: &[u8], binary: &str, destination: &mut File) -> Result<()> {
    let decoder = GzDecoder::new(Cursor::new(bytes));
    // Cap the decompressed stream as well as declared member sizes.
    let mut archive = tar::Archive::new(decoder.take(MAX_EXPANDED + 1));
    let mut expanded = 0_u64;
    let mut found = false;
    for (index, entry) in archive.entries()?.enumerate() {
        ensure!(index < MAX_ENTRIES, "archive has too many members");
        let mut entry = entry?;
        let name = safe_member(entry.path_bytes().as_ref())?;
        ensure!(
            entry.header().entry_type().is_file(),
            "archive member is not a regular file"
        );
        let size = entry.size();
        expanded = expanded
            .checked_add(size)
            .context("expanded archive size overflow")?;
        ensure!(
            expanded <= MAX_EXPANDED,
            "expanded archive exceeds its size limit"
        );
        if name == binary {
            ensure!(!found, "archive contains duplicate executables");
            copy_binary(&mut entry, destination, size)?;
            found = true;
        }
    }
    ensure!(found, "archive executable missing");
    Ok(())
}

fn hash_file(path: &Path) -> Result<String> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.file_type().is_file(),
        "staged executable is not a regular file"
    );
    ensure!(
        metadata.len() > 0 && metadata.len() <= MAX_BINARY,
        "invalid staged executable size"
    );
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

/// Replace the running image on disk. Existing processes retain their mapped image.
/// Windows returns its retained old image so the caller can clean it after sessions exit.
pub fn install(prepared: &Prepared) -> Result<Option<PathBuf>> {
    let executable = std::env::current_exe()?.canonicalize()?;
    install_to(prepared, &executable)
}

fn install_to(prepared: &Prepared, executable: &Path) -> Result<Option<PathBuf>> {
    let version = Version::parse(&prepared.version)?;
    ensure!(version.pre.is_empty(), "staged version is not stable");
    ensure!(
        version
            .cmp_precedence(&Version::parse(crate::VERSION)?)
            .is_gt(),
        "staged version is not newer"
    );
    let parent = executable
        .parent()
        .context("executable has no parent directory")?;
    let existing = fs::symlink_metadata(executable)?;
    ensure!(
        existing.file_type().is_file(),
        "installed executable is not a regular file"
    );
    // The complete new image is prepared beside the destination before any rename.
    let mut replacement = Builder::new().prefix(".xnip-new-").tempfile_in(parent)?;
    let staged = fs::symlink_metadata(&prepared.path)?;
    ensure!(
        staged.file_type().is_file(),
        "staged executable is not a regular file"
    );
    ensure!(
        staged.len() > 0 && staged.len() <= MAX_BINARY,
        "invalid staged executable size"
    );
    let mut source = File::open(&prepared.path)?;
    let copied = io::copy(
        &mut Read::by_ref(&mut source).take(MAX_BINARY + 1),
        replacement.as_file_mut(),
    )?;
    ensure!(
        copied > 0 && copied <= MAX_BINARY,
        "invalid staged executable size"
    );
    replacement.flush()?;
    replacement.as_file().sync_all()?;
    ensure!(
        hash_file(replacement.path())? == prepared.sha256,
        "staged executable SHA256 mismatch"
    );
    fs::set_permissions(replacement.path(), existing.permissions())?;
    replace_image(replacement, executable)
}

#[cfg(not(windows))]
fn replace_image(replacement: NamedTempFile, executable: &Path) -> Result<Option<PathBuf>> {
    replacement.persist(executable)?;
    Ok(None)
}

#[cfg(windows)]
fn replace_image(replacement: NamedTempFile, executable: &Path) -> Result<Option<PathBuf>> {
    // A mapped Windows executable can be renamed, but cannot be overwritten/deleted.
    // Keep the rename interval minimal and restore the original on a failed handover.
    let parent = executable
        .parent()
        .context("executable has no parent directory")?;
    let reservation = Builder::new()
        .prefix(".xnip-old-")
        .suffix(".exe")
        .tempfile_in(parent)?;
    let (_, backup) = reservation.keep()?;
    fs::remove_file(&backup)?;
    let replacement = replacement.into_temp_path();
    fs::rename(executable, &backup).context("cannot preserve old executable")?;
    if let Err(error) = fs::rename(&replacement, executable) {
        if let Err(rollback) = fs::rename(&backup, executable) {
            bail!(
                "executable handover failed: {error}; restore failed: {rollback}; old image retained at {}",
                backup.display()
            );
        }
        return Err(error).context("executable handover failed; original restored");
    }
    if fs::remove_file(&backup).is_ok() {
        Ok(None)
    } else {
        Ok(Some(backup))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use serde_json::json;
    use std::collections::HashMap;
    use std::io::{BufRead, Write};
    use std::net::TcpListener;
    use std::thread;

    fn zip_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, data) in entries {
            archive
                .start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            archive.write_all(data).unwrap();
        }
        archive.finish().unwrap().into_inner()
    }

    fn tar_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let compressed = GzEncoder::new(Vec::new(), Compression::fast());
        let mut archive = tar::Builder::new(compressed);
        for (name, data) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            archive.append_data(&mut header, name, *data).unwrap();
        }
        archive.into_inner().unwrap().finish().unwrap()
    }

    fn with_release(
        tag: &str,
        target: &str,
        archive: Vec<u8>,
        checksum_body: Option<Vec<u8>>,
        mutate: impl FnOnce(&mut serde_json::Value),
        run: impl FnOnce(DeliveryConfig) -> Result<Option<Prepared>>,
    ) -> Result<Option<Prepared>> {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let extension = if target.ends_with("windows-msvc") {
            "zip"
        } else {
            "tar.gz"
        };
        let name = format!("xnip-{tag}-{target}.{extension}");
        let checksum_body =
            checksum_body.unwrap_or_else(|| format!("{}  {name}\n", sha256(&archive)).into_bytes());
        let mut metadata = json!({
            "tag_name": tag, "draft": false, "prerelease": false,
            "assets": [
                {"name": name, "browser_download_url": format!("{origin}/{name}"), "size": archive.len()},
                {"name": format!("{name}.sha256"), "browser_download_url": format!("{origin}/{name}.sha256"), "size": checksum_body.len()}
            ]
        });
        mutate(&mut metadata);
        let routes = HashMap::from([
            ("/latest".to_owned(), serde_json::to_vec(&metadata).unwrap()),
            (format!("/{name}"), archive),
            (format!("/{name}.sha256"), checksum_body),
        ]);
        listener.set_nonblocking(true).unwrap();
        let (stop, stopped) = std::sync::mpsc::channel();
        let server = thread::spawn(move || {
            while stopped.try_recv().is_err() {
                match listener.accept() {
                    Ok((mut connection, _)) => {
                        // Windows accepted sockets inherit the listener\'s nonblocking mode.
                        connection.set_nonblocking(false).unwrap();
                        connection
                            .set_read_timeout(Some(Duration::from_secs(1)))
                            .unwrap();
                        let mut request = String::new();
                        {
                            let mut reader = std::io::BufReader::new(&mut connection);
                            loop {
                                let mut line = String::new();
                                if reader.read_line(&mut line).unwrap_or(0) == 0 {
                                    break;
                                }
                                request.push_str(&line);
                                if line == "\r\n" {
                                    break;
                                }
                            }
                        }
                        let path = request.split_whitespace().nth(1).unwrap_or("");
                        let Some(body) = routes.get(path) else {
                            continue;
                        };
                        let header = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = connection.write_all(header.as_bytes());
                        let _ = connection.write_all(body);
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(error) => panic!("{error}"),
                }
            }
        });
        let result = run(DeliveryConfig {
            release_url: format!("{origin}/latest"),
            proxy: None,
            target: target.to_owned(),
            current_version: "0.2.0".to_owned(),
        });
        let _ = stop.send(());
        server.join().unwrap();
        result
    }

    /// A real local forwarding proxy, with response corruption at a selected stage.
    fn with_proxy(
        config: &DeliveryConfig,
        fault: u8,
        run: impl FnOnce(DeliveryConfig) -> Result<Option<Prepared>>,
    ) -> (Result<Option<Prepared>>, Vec<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let proxy = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let (stop, stopped) = std::sync::mpsc::channel();
        let server = thread::spawn(move || {
            let mut requests = Vec::new();
            while stopped.try_recv().is_err() {
                match listener.accept() {
                    Ok((mut connection, _)) => {
                        connection.set_nonblocking(false).unwrap();
                        connection
                            .set_read_timeout(Some(Duration::from_secs(1)))
                            .unwrap();
                        let mut reader = std::io::BufReader::new(&mut connection);
                        let mut request = String::new();
                        reader.read_line(&mut request).unwrap();
                        loop {
                            let mut line = String::new();
                            if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                                break;
                            }
                        }
                        let url = request
                            .split_whitespace()
                            .nth(1)
                            .unwrap()
                            .strip_prefix('/')
                            .unwrap();
                        requests.push(url.to_owned());
                        let (status, mut body) = if fault == 1 {
                            (503, b"unavailable".to_vec())
                        } else {
                            let mut response = ureq::get(url).call().unwrap();
                            let mut body = Vec::new();
                            response
                                .body_mut()
                                .as_reader()
                                .take(MAX_ARCHIVE + 1)
                                .read_to_end(&mut body)
                                .unwrap();
                            (200, body)
                        };
                        if fault == 2 && url.ends_with("/latest") {
                            body = b"not release JSON".to_vec();
                        } else if fault == 3 && url.ends_with(".sha256") {
                            body[0] = if body[0] == b'0' { b'1' } else { b'0' };
                        } else if fault == 4
                            && Path::new(url)
                                .extension()
                                .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
                        {
                            body[0] ^= 1;
                        } else if fault == 5 && url.ends_with("/latest") {
                            let mut metadata: serde_json::Value =
                                serde_json::from_slice(&body).unwrap();
                            metadata["assets"][0]["browser_download_url"] =
                                json!("https://evil.invalid/xnip.zip");
                            body = serde_json::to_vec(&metadata).unwrap();
                        }
                        let header = format!(
                            "HTTP/1.1 {status} Result\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = connection.write_all(header.as_bytes());
                        let _ = connection.write_all(&body);
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(error) => panic!("{error}"),
                }
            }
            requests
        });
        let mut routed = config.clone();
        routed.proxy = Some(proxy);
        let result = run(routed);
        let _ = stop.send(());
        (result, server.join().unwrap())
    }

    #[test]
    fn proxy_precedes_direct_and_validation_failures_retry_the_entire_delivery() {
        for fault in 0..=5 {
            let directory = tempfile::tempdir().unwrap();
            let mut trace = DeliveryTrace::default();
            let mut requests = Vec::new();
            let prepared = with_release(
                "v0.3.0",
                "x86_64-pc-windows-msvc",
                zip_bytes(&[("xnip.exe", b"verified executable")]),
                None,
                |_| {},
                |config| {
                    let (result, observed) = with_proxy(&config, fault, |routed| {
                        prepare_traced(&routed, directory.path(), &mut trace)
                    });
                    requests = observed;
                    result
                },
            )
            .unwrap()
            .unwrap();
            assert_eq!(fs::read(&prepared.path).unwrap(), b"verified executable");
            assert_eq!(
                trace.route.as_deref(),
                Some(if fault == 0 { "proxy" } else { "direct" })
            );
            assert_eq!(trace.proxy_error.is_some(), fault != 0);
            assert_eq!(
                requests.len(),
                if matches!(fault, 0 | 3 | 4) { 3 } else { 1 }
            );
            assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
        }
    }

    #[test]
    fn proxy_configuration_accepts_only_https_origins_and_an_explicit_direct_mode() {
        assert_eq!(
            DeliveryConfig::default().proxy.as_deref(),
            Some(DEFAULT_PROXY)
        );
        for direct in ["0", "direct"] {
            assert_eq!(normalize_proxy(direct).unwrap(), None);
        }
        assert_eq!(
            normalize_proxy("https://mirror.example:8443/")
                .unwrap()
                .as_deref(),
            Some("https://mirror.example:8443")
        );
        for invalid in [
            "",
            "http://mirror.example",
            "https://user:secret@mirror.example",
            "https://mirror.example/path",
            "https://mirror.example?q=x",
            "https://mirror.example#x",
            "https://",
        ] {
            assert!(normalize_proxy(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn body_read_timeout_is_capped_by_the_remaining_delivery_budget() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/latest", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            let (mut connection, _) = listener.accept().unwrap();
            let mut request = [0; 4096];
            let _ = connection.read(&mut request);
            connection
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1\r\nConnection: close\r\n\r\n")
                .unwrap();
            thread::sleep(Duration::from_secs(1));
            let _ = connection.write_all(b"x");
        });
        let mut fetch = Fetch::new(&url).unwrap();
        fetch.deadline = Instant::now() + Duration::from_millis(100);
        let started = Instant::now();
        assert!(fetch.get(&url, 10).is_err());
        assert!(started.elapsed() < Duration::from_millis(750));
        server.join().unwrap();
    }

    #[test]
    fn stages_checksum_verified_zip_and_tar_without_executing() {
        for (target, archive) in [
            (
                "x86_64-pc-windows-msvc",
                zip_bytes(&[("xnip.exe", b"new executable")]),
            ),
            (
                "x86_64-unknown-linux-musl",
                tar_bytes(&[("xnip", b"new executable"), ("LICENSE", b"MIT")]),
            ),
        ] {
            let directory = tempfile::tempdir().unwrap();
            let prepared = with_release(
                "v0.3.0",
                target,
                archive,
                None,
                |_| {},
                |config| prepare(&config, directory.path()),
            )
            .unwrap()
            .unwrap();
            assert_eq!(prepared.version, "0.3.0");
            assert_eq!(fs::read(&prepared.path).unwrap(), b"new executable");
            assert_eq!(prepared.sha256, sha256(b"new executable"));
        }
    }

    #[test]
    fn unchanged_or_older_versions_do_not_download_assets_or_create_state() {
        for tag in ["v0.1.0", "v0.2.0", "v0.2.0+build"] {
            let directory = tempfile::tempdir().unwrap();
            let absent = directory.path().join("not-created");
            let result = with_release(
                tag,
                "x86_64-pc-windows-msvc",
                vec![1],
                None,
                |_| {},
                |config| prepare(&config, &absent),
            );
            assert!(result.unwrap().is_none());
            assert!(!absent.exists());
        }
    }

    #[test]
    fn source_builds_select_published_releases_for_the_same_os_and_cpu() {
        for (build, published) in [
            ("x86_64-unknown-linux-gnu", "x86_64-unknown-linux-musl"),
            ("aarch64-unknown-linux-gnu", "aarch64-unknown-linux-musl"),
            ("x86_64-unknown-linux-musl", "x86_64-unknown-linux-musl"),
            ("aarch64-unknown-linux-musl", "aarch64-unknown-linux-musl"),
            ("x86_64-pc-windows-msvc", "x86_64-pc-windows-msvc"),
            ("aarch64-pc-windows-msvc", "aarch64-pc-windows-msvc"),
            ("x86_64-apple-darwin", "x86_64-apple-darwin"),
            ("aarch64-apple-darwin", "aarch64-apple-darwin"),
        ] {
            assert_eq!(release_target(build), published);
            assert!(target_binary(release_target(build)).is_ok());
        }
        for unsupported in [
            "i686-unknown-linux-gnu",
            "x86_64-unknown-linux-gnux32",
            "x86_64-unknown-freebsd",
            "x86_64-pc-windows-gnu",
        ] {
            assert_eq!(release_target(unsupported), unsupported);
            assert!(target_binary(release_target(unsupported)).is_err());
        }
    }

    #[test]
    fn rejects_unstable_or_malformed_versions_and_unsupported_targets() {
        let directory = tempfile::tempdir().unwrap();
        for tag in ["v0.3.0-beta.1", "../evil", "latest"] {
            assert!(
                with_release(
                    tag,
                    "x86_64-pc-windows-msvc",
                    vec![1],
                    None,
                    |_| {},
                    |config| prepare(&config, directory.path())
                )
                .is_err()
            );
        }
        let unsupported = DeliveryConfig {
            target: "unknown".to_owned(),
            ..DeliveryConfig::default()
        };
        assert!(prepare(&unsupported, directory.path()).is_err());
    }

    #[test]
    fn rejects_draft_prerelease_missing_duplicate_and_misnamed_assets() {
        for mutation in 0..7 {
            let directory = tempfile::tempdir().unwrap();
            let result = with_release(
                "v0.3.0",
                "x86_64-pc-windows-msvc",
                zip_bytes(&[("xnip.exe", b"new")]),
                None,
                |metadata| match mutation {
                    0 => metadata["draft"] = json!(true),
                    1 => metadata["prerelease"] = json!(true),
                    2 => metadata["assets"] = json!([]),
                    3 => {
                        let duplicate = metadata["assets"][0].clone();
                        metadata["assets"].as_array_mut().unwrap().push(duplicate);
                    }
                    4 => {
                        metadata["assets"][0]["browser_download_url"] =
                            json!("https://example.com/evil");
                    }
                    5 => metadata["assets"][0]["size"] = json!(MAX_ARCHIVE + 1),
                    _ => {
                        metadata["assets"][0]["digest"] =
                            json!(format!("sha256:{}", "0".repeat(64)));
                    }
                },
                |config| prepare(&config, directory.path()),
            );
            assert!(result.is_err(), "mutation {mutation}");
            assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
        }
    }

    #[test]
    fn rejects_corrupt_checksum_and_archive_size_mismatch() {
        for mutation in 0..4 {
            let directory = tempfile::tempdir().unwrap();
            let checksum = match mutation {
                0 => Some(b"not-a-sha256\n".to_vec()),
                1 => Some(format!("{}  wrong.zip\n", "0".repeat(64)).into_bytes()),
                2 => Some(
                    format!(
                        "{}  xnip-v0.3.0-x86_64-pc-windows-msvc.zip\n",
                        "0".repeat(64)
                    )
                    .into_bytes(),
                ),
                _ => None,
            };
            let result = with_release(
                "v0.3.0",
                "x86_64-pc-windows-msvc",
                zip_bytes(&[("xnip.exe", b"new")]),
                checksum,
                |metadata| {
                    if mutation == 3 {
                        metadata["assets"][0]["size"] = json!(1);
                    }
                },
                |config| prepare(&config, directory.path()),
            );
            assert!(result.is_err());
            assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
        }
    }

    #[test]
    fn archive_rejects_missing_duplicate_and_unsafe_members_without_leaking_staged_files() {
        for archive in [
            zip_bytes(&[("README.md", b"missing")]),
            zip_bytes(&[("../xnip.exe", b"escape")]),
            zip_bytes(&[("xnip.exe", b"one"), ("./xnip.exe", b"two")]),
            zip_bytes(&[("xnip.exe", b"one"), ("C:/escape", b"bad")]),
        ] {
            let directory = tempfile::tempdir().unwrap();
            assert!(
                with_release(
                    "v0.3.0",
                    "x86_64-pc-windows-msvc",
                    archive,
                    None,
                    |_| {},
                    |config| prepare(&config, directory.path())
                )
                .is_err()
            );
            assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
        }
    }

    #[test]
    fn archive_rejects_links_large_entries_and_too_many_members() {
        let mut destination = tempfile::tempfile().unwrap();
        let compressed = GzEncoder::new(Vec::new(), Compression::fast());
        let mut archive = tar::Builder::new(compressed);
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_size(0);
        header.set_mode(0o755);
        header.set_cksum();
        archive.append_link(&mut header, "xnip", "/bin/sh").unwrap();
        let bytes = archive.into_inner().unwrap().finish().unwrap();
        assert!(extract_tar(&bytes, "xnip", &mut destination).is_err());

        let entries: Vec<_> = (0..=MAX_ENTRIES)
            .map(|_| ("README.md", b"tiny".as_slice()))
            .collect();
        let bytes = tar_bytes(&entries);
        assert!(extract_tar(&bytes, "xnip", &mut destination).is_err());
        assert!(copy_binary(&mut Cursor::new(b"small"), &mut destination, MAX_BINARY + 1).is_err());
    }

    #[test]
    fn paths_and_endpoints_reject_escape_and_remote_overrides() {
        for path in [
            "../xnip",
            "/xnip",
            "C:\\xnip",
            "a/../xnip",
            "a//xnip",
            "a\\xnip",
            "xnip\0evil",
        ] {
            assert!(safe_member(path.as_bytes()).is_err(), "{path:?}");
        }
        assert_eq!(safe_member(b"./xnip").unwrap(), "xnip");
        for url in [
            "http://evil.test:80/latest",
            "http://127.0.0.1:1@evil.test:80/latest",
            "https://example.com/latest",
            "http://localhost:0/latest",
        ] {
            assert!(Fetch::new(url).is_err(), "{url}");
        }
    }

    #[test]
    fn bounded_http_rejects_large_declared_and_actual_body() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/latest", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            for response in [
                b"HTTP/1.1 200 OK\r\nContent-Length: 20\r\n\r\n".as_slice(),
                b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n12345678901234567890".as_slice(),
            ] {
                let (mut connection, _) = listener.accept().unwrap();
                let mut request = [0; 1024];
                let _ = connection.read(&mut request);
                let _ = connection.write_all(response);
            }
        });
        let fetch = Fetch::new(&url).unwrap();
        assert!(fetch.get(&url, 10).is_err());
        assert!(fetch.get(&url, 10).is_err());
        server.join().unwrap();
    }

    #[test]
    fn expired_delivery_deadline_performs_no_http_call() {
        let fetch = Fetch {
            agent: ureq::Agent::new_with_defaults(),
            fixture_origin: None,
            proxy: None,
            deadline: Instant::now().checked_sub(Duration::from_secs(1)).unwrap(),
        };
        assert!(fetch.get("https://example.invalid/", 10).is_err());
    }

    #[test]
    fn hash_and_install_reject_empty_candidates_and_downgrades() {
        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("xnip");
        let staged = directory.path().join("candidate");
        fs::write(&executable, b"old").unwrap();
        fs::write(&staged, b"").unwrap();
        assert!(hash_file(&staged).is_err());
        let prepared = Prepared {
            version: "999.0.0".to_owned(),
            path: staged.clone(),
            sha256: sha256(b""),
        };
        assert!(install_to(&prepared, &executable).is_err());
        fs::write(&staged, b"new").unwrap();
        for version in ["0.0.0", crate::VERSION] {
            let prepared = Prepared {
                version: version.to_owned(),
                path: staged.clone(),
                sha256: sha256(b"new"),
            };
            assert!(install_to(&prepared, &executable).is_err(), "{version}");
            assert_eq!(fs::read(&executable).unwrap(), b"old");
        }
    }

    #[test]
    fn install_verifies_the_copied_image_and_preserves_original_on_failure() {
        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("xnip");
        let staged = directory.path().join("candidate");
        fs::write(&executable, b"old image").unwrap();
        fs::write(&staged, b"new image").unwrap();
        let mut prepared = Prepared {
            version: "999.0.0".to_owned(),
            path: staged.clone(),
            sha256: sha256(b"wrong"),
        };
        assert!(install_to(&prepared, &executable).is_err());
        assert_eq!(fs::read(&executable).unwrap(), b"old image");
        prepared.sha256 = sha256(b"new image");
        let retained = install_to(&prepared, &executable).unwrap();
        assert_eq!(fs::read(&executable).unwrap(), b"new image");
        assert_eq!(fs::read(staged).unwrap(), b"new image");
        if let Some(old) = retained {
            assert_eq!(fs::read(old).unwrap(), b"old image");
        }
    }

    #[cfg(windows)]
    #[test]
    fn mapped_image_child() {
        use std::io::BufRead;
        if std::env::var_os("XNIP_DELIVERY_CHILD").is_none() {
            return;
        }
        println!("image-ready");
        io::stdout().flush().unwrap();
        for line in io::stdin().lock().lines() {
            match line.unwrap().as_str() {
                "ping" => {
                    println!("image-alive");
                    io::stdout().flush().unwrap();
                }
                "exit" => break,
                command => panic!("unexpected child command: {command}"),
            }
        }
    }

    #[cfg(windows)]
    #[test]
    fn replacing_a_mapped_windows_image_keeps_its_process_alive_and_new_launches_work() {
        use std::io::BufRead;
        use std::process::{Command, Stdio};
        use std::sync::mpsc;
        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("xnip.exe");
        let staged = directory.path().join("candidate.exe");
        let test_image = std::env::current_exe().unwrap();
        fs::copy(&test_image, &executable).unwrap();
        fs::copy(&test_image, &staged).unwrap();
        fs::OpenOptions::new()
            .append(true)
            .open(&staged)
            .unwrap()
            .write_all(b"next-launch-image")
            .unwrap();
        let prepared = Prepared {
            version: "999.0.0".to_owned(),
            path: staged.clone(),
            sha256: hash_file(&staged).unwrap(),
        };
        let mut child = Command::new(&executable)
            .args([
                "--exact",
                "update::delivery::tests::mapped_image_child",
                "--nocapture",
            ])
            .env("XNIP_DELIVERY_CHILD", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let output = child.stdout.take().unwrap();
        let (sender, received) = mpsc::channel();
        let reader = thread::spawn(move || {
            for line in io::BufReader::new(output).lines() {
                if sender.send(line.unwrap()).is_err() {
                    break;
                }
            }
        });
        let expect = |text: &str| {
            loop {
                let line = received.recv_timeout(Duration::from_secs(10)).unwrap();
                if line == text {
                    break;
                }
            }
        };
        expect("image-ready");
        let backup = install_to(&prepared, &executable).unwrap().unwrap();
        assert_eq!(hash_file(&executable).unwrap(), prepared.sha256);
        assert!(
            backup.exists(),
            "Windows must retain the still-mapped old image"
        );
        writeln!(child.stdin.as_mut().unwrap(), "ping").unwrap();
        child.stdin.as_mut().unwrap().flush().unwrap();
        expect("image-alive");
        assert!(child.try_wait().unwrap().is_none());
        let new_launch = Command::new(&executable)
            .args(["--exact", "update::delivery::tests::mapped_image_child"])
            .env_remove("XNIP_DELIVERY_CHILD")
            .output()
            .unwrap();
        assert!(new_launch.status.success(), "{new_launch:?}");
        writeln!(child.stdin.as_mut().unwrap(), "exit").unwrap();
        child.stdin.take();
        assert!(child.wait().unwrap().success());
        reader.join().unwrap();
        fs::remove_file(backup).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn installation_preserves_executable_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("xnip");
        let staged = directory.path().join("candidate");
        fs::write(&executable, b"old").unwrap();
        fs::write(&staged, b"new").unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o751)).unwrap();
        install_to(
            &Prepared {
                version: "999.0.0".to_owned(),
                path: staged,
                sha256: sha256(b"new"),
            },
            &executable,
        )
        .unwrap();
        assert_eq!(
            fs::metadata(executable).unwrap().permissions().mode() & 0o777,
            0o751
        );
    }
}
