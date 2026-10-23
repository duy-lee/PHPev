//! MariaDB as a supervised process.

use std::io;
use std::process::Stdio;
use std::time::Duration;

use phpev_core::layout::Layout;
use phpev_core::{ipc, mariadb, procinfo};
use tokio::process::{Child, Command};

use crate::log::{log_info, log_warn};
use crate::supervise::{wait_listening, Supervised};

pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const MARIADB_READY_TIMEOUT: Duration = Duration::from_secs(30);
/// Time MariaDB gets to flush and exit after `mariadb-admin shutdown`.
const MARIADB_STOP_GRACE: Duration = Duration::from_secs(15);

// ---------------------------------------------------------------- MariaDB

pub struct Db {
    pub version: String,
    pub port: u16,
    sup: Supervised,
}

impl Db {
    pub async fn start(layout: &Layout, version: &str, port: u16) -> io::Result<Self> {
        let (l, v) = (layout.clone(), version.to_owned());
        // mariadb-install-db is synchronous and takes a few seconds on first use.
        tokio::task::spawn_blocking(move || mariadb::init_data_dir(&l, &v))
            .await
            .map_err(io::Error::other)??;

        let child = spawn_mariadb(layout, version, port).await?;
        let (l, v) = (layout.clone(), version.to_owned());
        let respawn = move || {
            let (l, v) = (l.clone(), v.clone());
            async move { spawn_mariadb(&l, &v, port).await.map(|c| (c, port)) }
        };
        let (l, v) = (layout.clone(), version.to_owned());
        let graceful = move |port: u16| async move {
            let r = tokio::task::spawn_blocking(move || mariadb::admin_shutdown(&l, &v, port)).await;
            if let Ok(Err(e)) = r {
                log_warn!("mariadb-admin shutdown failed: {e}");
            }
        };
        let sup = Supervised::new(
            format!("MariaDB {version}"),
            child,
            port,
            MARIADB_STOP_GRACE,
            respawn,
            graceful,
        );
        log_info!("MariaDB {version} running on 127.0.0.1:{port}");
        Ok(Self {
            version: version.to_owned(),
            port,
            sup,
        })
    }

    pub async fn stop(self) {
        self.sup.stop().await;
    }

    pub fn status(&self) -> ipc::Service {
        let pid = self.sup.shared.pid();
        ipc::Service {
            version: self.version.clone(),
            port: self.port,
            pid,
            memory_kb: pid.and_then(procinfo::memory_kb),
            state: self.sup.shared.state().to_owned(),
        }
    }
}

async fn spawn_mariadb(layout: &Layout, version: &str, port: u16) -> io::Result<Child> {
    let mut child = Command::new(mariadb::server_exe(layout, version))
        .args(mariadb::server_args(layout, version, port))
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    if let Err(e) = wait_listening(&mut child, port, MARIADB_READY_TIMEOUT).await {
        let log = layout
            .logs_dir()
            .join(format!("mariadb-{}.err", mariadb::major_of(version)));
        return Err(io::Error::new(
            e.kind(),
            format!("MariaDB {version} {e}; see {}", log.display()),
        ));
    }
    Ok(child)
}
