//! Keeps the tray's icons across a restart of the shell.
//!
//! The first shell of a session owns `org.kde.StatusNotifierWatcher`, and the
//! tray programs register with it. kded6, once something has started it,
//! queues for the same name. So when the shell exits (a crash, or a restart
//! to pick up a new build) the bus hands the name straight to kded6, whose
//! list is empty. Programs only register again when the name appears from
//! having no owner at all: Qt's tray icons and KDE's (cmst, Dolphin and the
//! like) take no notice of a hand-over, stay registered with a watcher that
//! is gone, and are missing from the next shell's tray until they are
//! restarted. (libayatana and Chromium do register again by themselves. The
//! Quickshell shell lost its tray in the same way.)
//!
//! kded6 cannot be told to let go of the name, and its watcher takes a
//! registration from anyone on an item's behalf. So the shell writes down
//! the items it shows, and one that starts under somebody else's watcher
//! registers the ones that watcher does not know.
//!
//! Only items at `/StatusNotifierItem` under a unique bus name can be
//! registered for someone else (the watcher supplies that path itself), which
//! is what Qt and KDE items are. Unique names are never reused on a bus, and
//! the item's `Id` is checked before it is registered, so an old entry can
//! only fail.
//!
//! What a rescue did is in the shell's log: a line at every start, so that
//! silence means a shell without this code, and one for each item it had to
//! act on. Nothing else tells a watcher that ignored the call from a program
//! that did not answer.

use anti_webui::shell::TrayItem;
use std::collections::HashSet;
use std::future::Future;
use std::io::Write;
use std::path::PathBuf;
use std::pin::Pin;
use std::task::Poll;
use std::time::Duration;
use zbus::Connection;
use zbus::zvariant::OwnedValue;

const FILE_NAME: &str = "minkashell-tray-items";
const BUS: &str = "org.freedesktop.DBus";
const BUS_PATH: &str = "/org/freedesktop/DBus";
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";
const WATCHER: &str = "org.kde.StatusNotifierWatcher";
const WATCHER_PATH: &str = "/StatusNotifierWatcher";
const ITEM_PATH: &str = "/StatusNotifierItem";
const ITEM_INTERFACES: [&str; 2] = [
    "org.kde.StatusNotifierItem",
    "org.freedesktop.StatusNotifierItem",
];
/// More than any tray holds. Entries are only ever added during a run, one
/// for each program that showed an icon, restarts of it included; over the
/// limit, the ones shown longest ago go.
const MAX_SAVED: usize = 128;
/// A program that is stopped or swapped out answers late, and one that has
/// only just taken the watcher's name has no watcher object yet: the items
/// still unanswered are tried again after each of these waits, in seconds.
const PASSES: [u64; 3] = [0, 5, 10];
const CALL_TIMEOUT: Duration = Duration::from_secs(10);

/// An item's unique bus name and its `Id`.
pub type Saved = Vec<(String, String)>;

/// A line for the shell's log. Not `eprintln!`, which panics when the log
/// cannot be written to.
fn note(text: std::fmt::Arguments) {
    let _ = writeln!(std::io::stderr(), "minkashell: tray rescue: {text}");
}

fn saved_path() -> Option<PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR")
        .filter(|dir| !dir.is_empty())
        .map(|dir| PathBuf::from(dir).join(FILE_NAME))
}

/// The bus name in an item's key, if the item is one that can be registered
/// on its behalf: `:1.868/StatusNotifierItem` gives `:1.868`.
fn rescuable(key: &str) -> Option<&str> {
    let (service, path) = key.split_at(key.find('/')?);
    (service.starts_with(':') && path == ITEM_PATH).then_some(service)
}

/// What to write down for the items on show.
pub fn snapshot(items: &[TrayItem]) -> Saved {
    items
        .iter()
        .filter_map(|item| {
            let key = item.key();
            let service = rescuable(&key)?;
            // One line an item, the two fields tab-separated.
            let plain = !item.id.is_empty() && !item.id.contains(['\t', '\n']);
            plain.then(|| (service.to_owned(), item.id.clone()))
        })
        .collect()
}

/// `saved` with the items of `now` moved to its end, so the list runs from
/// the item shown longest ago to the ones on show. Nothing is dropped for
/// not being on show: an item that has not come back yet has to still be in
/// the file at the next start. Over the limit the front goes, which is never
/// an item on show.
pub fn merge(saved: &Saved, now: Saved) -> Saved {
    let mut merged = saved.clone();
    for (service, id) in now {
        merged.retain(|(known, _)| *known != service);
        merged.push((service, id));
    }
    let extra = merged.len().saturating_sub(MAX_SAVED);
    merged.drain(..extra);
    merged
}

fn parse(text: &str) -> Saved {
    text.lines()
        .filter_map(|line| {
            let (service, id) = line.split_once('\t')?;
            (service.starts_with(':') && !id.is_empty())
                .then(|| (service.to_owned(), id.to_owned()))
        })
        .collect()
}

fn render(saved: &Saved) -> String {
    saved
        .iter()
        .map(|(service, id)| format!("{service}\t{id}\n"))
        .collect()
}

pub fn load() -> Saved {
    saved_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map(|text| parse(&text))
        .unwrap_or_default()
}

/// Replaces the file in one step, so a shell that dies while writing leaves
/// the old list and not half of the new one. False when the file is not the
/// new list, for the caller to try again. With no runtime directory there is
/// nowhere to write and nothing to try again.
pub fn store(saved: &Saved) -> bool {
    let Some(path) = saved_path() else {
        return true;
    };
    let partial = path.with_extension("partial");
    std::fs::write(&partial, render(saved)).is_ok() && std::fs::rename(&partial, &path).is_ok()
}

/// Registers the saved items the current watcher lacks, from a thread of its
/// own: the calls wait on other programs, some for seconds.
pub fn rescue_in_background(saved: Saved) {
    if saved.is_empty() {
        return note(format_args!("nothing saved"));
    }
    let spawned = std::thread::Builder::new()
        .name("minkashell-tray-rescue".into())
        .spawn(move || {
            let connection = zbus::block_on(async {
                zbus::connection::Builder::session()?
                    .method_timeout(CALL_TIMEOUT)
                    .build()
                    .await
            });
            let connection = match connection {
                Ok(connection) => connection,
                Err(error) => return note(format_args!("no session bus: {error}")),
            };
            let mut pending = saved;
            let mut asked = false;
            let mut unread: Option<String> = None;
            for wait in PASSES {
                std::thread::sleep(Duration::from_secs(wait));
                match zbus::block_on(pass(&connection, &pending, std::process::id())) {
                    Pass::Nothing => return,
                    Pass::Rest(rest) if rest.is_empty() => return,
                    Pass::Rest(rest) => {
                        pending = rest;
                        asked = true;
                    }
                    // Said at once, and once: a shell that exits before the
                    // last pass has still left the reason.
                    Pass::Unread(why) => {
                        if unread.as_ref() != Some(&why) {
                            note(format_args!("cannot read the watcher's list: {why}"));
                            unread = Some(why);
                        }
                    }
                }
            }
            if asked {
                for (service, id) in &pending {
                    note(format_args!("gave up on {service} ({id})"));
                }
            } else {
                // Not a line an entry: most are of programs long gone, and
                // none of them was tried.
                note(format_args!(
                    "gave up: the watcher's list could not be read ({} saved, none asked)",
                    pending.len()
                ));
            }
        });
    if let Err(error) = spawned {
        note(format_args!("not started: {error}"));
    }
}

/// Runs the futures side by side and gives their results in order: a pass
/// takes as long as its slowest program, not as long as all of them.
async fn join_all<'a, T>(futures: Vec<Pin<Box<dyn Future<Output = T> + 'a>>>) -> Vec<T> {
    let mut running: Vec<_> = futures.into_iter().map(Some).collect();
    let mut results: Vec<Option<T>> = running.iter().map(|_| None).collect();
    std::future::poll_fn(|context| {
        for (slot, result) in running.iter_mut().zip(&mut results) {
            if let Some(future) = slot
                && let Poll::Ready(value) = future.as_mut().poll(context)
            {
                *result = Some(value);
                *slot = None;
            }
        }
        if running.iter().all(Option::is_none) {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    })
    .await;
    results.into_iter().flatten().collect()
}

async fn property(
    connection: &Connection,
    service: &str,
    path: &str,
    interface: &str,
    name: &str,
) -> zbus::Result<OwnedValue> {
    connection
        .call_method(
            Some(service),
            path,
            Some(PROPERTIES),
            "Get",
            &(interface, name),
        )
        .await?
        .body()
        .deserialize()
}

/// The unique bus names of the programs the watcher lists.
async fn listed(connection: &Connection) -> zbus::Result<HashSet<String>> {
    let entries = property(
        connection,
        WATCHER,
        WATCHER_PATH,
        WATCHER,
        "RegisteredStatusNotifierItems",
    )
    .await?;
    let entries = Vec::<String>::try_from(entries).map_err(zbus::Error::Variant)?;
    // Entries are a bus name followed by a path. An item that registered
    // itself under a well-known name is the same program as its unique one.
    let mut services = HashSet::new();
    for entry in &entries {
        let service = entry.split('/').next().unwrap_or(entry);
        if service.starts_with(':') {
            services.insert(service.to_owned());
        } else if let Ok(reply) = connection
            .call_method(Some(BUS), BUS_PATH, Some(BUS), "GetNameOwner", &(service,))
            .await
            && let Ok(unique) = reply.body().deserialize::<String>()
        {
            services.insert(unique);
        }
    }
    Ok(services)
}

/// What became of one saved item in a pass.
enum Outcome {
    /// The watcher was asked to register it. (Its reply says nothing: the
    /// call returns the same whether or not it took the item.)
    Asked,
    /// Its program has left the bus.
    Exited,
    /// Left alone for good, and why.
    Dropped(String),
    /// To be tried again, and why.
    Retry(String),
}

async fn rescue_one(connection: &Connection, service: &str, id: &str) -> Outcome {
    for interface in ITEM_INTERFACES {
        let found = match property(connection, service, ITEM_PATH, interface, "Id").await {
            Ok(value) => String::try_from(value).ok(),
            // An error for a reply. From the bus itself the program has
            // gone; from the program it only lacks this interface.
            Err(zbus::Error::MethodError(name, ..)) => {
                let name = name.as_str();
                if name.ends_with(".ServiceUnknown") || name.ends_with(".NameHasNoOwner") {
                    return Outcome::Exited;
                }
                None
            }
            // Nothing in time. It may only be busy.
            Err(error) => return Outcome::Retry(format!("no answer ({error})")),
        };
        match found {
            // Still the item that was on show: say so for it. The watcher
            // announces it, and the tray takes it from there.
            Some(found) if found == id => {
                let asked = connection
                    .call_method(
                        Some(WATCHER),
                        WATCHER_PATH,
                        Some(WATCHER),
                        "RegisterStatusNotifierItem",
                        &(service,),
                    )
                    .await;
                return match asked {
                    Ok(_) => Outcome::Asked,
                    Err(error) => Outcome::Retry(format!("the watcher refused it ({error})")),
                };
            }
            Some(found) => return Outcome::Dropped(format!("its item is \"{found}\" now")),
            None => {}
        }
    }
    Outcome::Dropped("it has no item now".to_owned())
}

/// How a round over the items still to register ended.
#[derive(Debug, PartialEq)]
enum Pass {
    /// Nothing for this shell to do: it is the watcher, or there is none.
    Nothing,
    /// The watcher did not give its list, so nothing was asked, and why.
    Unread(String),
    /// The items to try again.
    Rest(Saved),
}

/// One round over the items still to register. `me` is this process, to
/// know its own watcher by.
async fn pass(connection: &Connection, pending: &Saved, me: u32) -> Pass {
    // No watcher yet, or this shell's own: either way the name came from
    // having no owner, and every program registers for that by itself.
    let owner = connection
        .call_method(
            Some(BUS),
            BUS_PATH,
            Some(BUS),
            "GetConnectionUnixProcessID",
            &(WATCHER,),
        )
        .await
        .and_then(|reply| reply.body().deserialize::<u32>());
    match owner {
        Ok(owner) if owner == me => {
            note(format_args!("this shell is the watcher, nothing to tell"));
            return Pass::Nothing;
        }
        Ok(_) => {}
        Err(error) => {
            note(format_args!("no other watcher to tell ({error})"));
            return Pass::Nothing;
        }
    }

    // The watcher may have the name before it has its object: try again.
    let present = match listed(connection).await {
        Ok(present) => present,
        Err(error) => return Pass::Unread(error.to_string()),
    };
    let missing: Vec<&(String, String)> = pending
        .iter()
        .filter(|(service, _)| !present.contains(service))
        .collect();
    let outcomes = join_all(
        missing
            .iter()
            .map(|(service, id)| {
                Box::pin(rescue_one(connection, service, id))
                    as Pin<Box<dyn Future<Output = _> + '_>>
            })
            .collect(),
    )
    .await;

    // Whether the watcher took an item only shows in its list.
    let asked = outcomes
        .iter()
        .any(|outcome| matches!(outcome, Outcome::Asked));
    let now = if asked {
        listed(connection).await.map_err(|error| error.to_string())
    } else {
        Ok(HashSet::new())
    };
    let already = pending.len() - missing.len();
    let mut rest = Saved::new();
    let (mut registered, mut exited) = (0, 0);
    for ((service, id), outcome) in missing.into_iter().zip(outcomes) {
        match outcome {
            Outcome::Asked => match &now {
                Ok(now) if now.contains(service) => {
                    registered += 1;
                    note(format_args!("registered {service} ({id})"));
                }
                Ok(_) => {
                    note(format_args!(
                        "the watcher took the call for {service} ({id}) but does not list it"
                    ));
                    rest.push((service.clone(), id.clone()));
                }
                // Not known either way: the next pass reads the list anew.
                Err(error) => {
                    note(format_args!(
                        "{service} ({id}): asked the watcher, but its list could not be read \
                         ({error})"
                    ));
                    rest.push((service.clone(), id.clone()));
                }
            },
            Outcome::Exited => exited += 1,
            Outcome::Dropped(why) => note(format_args!("left {service} ({id}) alone: {why}")),
            Outcome::Retry(why) => {
                note(format_args!("{service} ({id}): {why}"));
                rest.push((service.clone(), id.clone()));
            }
        }
    }
    note(format_args!(
        "{already} already with the watcher, {registered} registered, {exited} of programs \
         that have exited, {} to try again",
        rest.len()
    ));
    Pass::Rest(rest)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn saved(entries: &[(&str, &str)]) -> Saved {
        entries
            .iter()
            .map(|(service, id)| ((*service).to_owned(), (*id).to_owned()))
            .collect()
    }

    #[test]
    fn only_unique_names_at_the_standard_path() {
        assert_eq!(rescuable(":1.868/StatusNotifierItem"), Some(":1.868"));
        // libayatana's path: the watcher would register the wrong object.
        assert_eq!(rescuable(":1.90/org/ayatana/NotificationItem/app"), None);
        // A well-known name may belong to another program by the next start.
        assert_eq!(
            rescuable("org.kde.StatusNotifierItem-12-1/StatusNotifierItem"),
            None
        );
        assert_eq!(rescuable(":1.868"), None);
    }

    #[test]
    fn file_round_trip() {
        let items = saved(&[(":1.868", "CMST - Connman System Tray"), (":1.9", "x")]);
        assert_eq!(parse(&render(&items)), items);
        // Anything that is not a line of this file is skipped.
        assert_eq!(
            parse("garbage\n:1.2\t\nname\tid\n:1.3\tok\n"),
            saved(&[(":1.3", "ok")])
        );
        assert!(parse("").is_empty());
    }

    #[test]
    fn merge_adds_and_never_forgets() {
        let before = saved(&[(":1.1", "a"), (":1.2", "b")]);
        // An empty tray, as just after a start, forgets nothing.
        assert_eq!(merge(&before, Saved::new()), before);
        // A new item is added after the ones known.
        assert_eq!(
            merge(&before, saved(&[(":1.2", "b"), (":1.3", "c")])),
            saved(&[(":1.1", "a"), (":1.2", "b"), (":1.3", "c")])
        );
        // An item on show moves to the end, and takes its new Id with it.
        assert_eq!(
            merge(&before, saved(&[(":1.1", "z")])),
            saved(&[(":1.2", "b"), (":1.1", "z")])
        );
        // Settled: the same tray again changes nothing, so nothing is
        // written a second time.
        let settled = merge(&before, saved(&[(":1.2", "b"), (":1.1", "a")]));
        assert_eq!(
            merge(&settled, saved(&[(":1.2", "b"), (":1.1", "a")])),
            settled
        );
    }

    #[test]
    fn merge_drops_the_longest_unseen() {
        let many: Saved = (0..MAX_SAVED + 5)
            .map(|n| (format!(":1.{n}"), "id".to_owned()))
            .collect();
        let merged = merge(&Saved::new(), many);
        assert_eq!(merged.len(), MAX_SAVED);
        assert_eq!(merged[0].0, ":1.5");
        assert_eq!(merged[MAX_SAVED - 1].0, format!(":1.{}", MAX_SAVED + 4));

        // At the limit, an item that has been on show all session outlives
        // the dead entries behind it when a new one arrives.
        let full: Saved = (0..MAX_SAVED)
            .map(|n| (format!(":1.{n}"), "id".to_owned()))
            .collect();
        let merged = merge(&full, saved(&[(":1.0", "id"), (":1.900", "new")]));
        assert_eq!(merged.len(), MAX_SAVED);
        assert!(merged.iter().any(|(service, _)| service == ":1.0"));
        assert!(merged.iter().any(|(service, _)| service == ":1.900"));
        assert!(!merged.iter().any(|(service, _)| service == ":1.1"));
    }

    /// A bus of its own for a test, so nothing here touches the session's.
    struct PrivateBus {
        daemon: std::process::Child,
        dir: PathBuf,
        address: String,
    }

    /// A bus with nothing of the session's in it. `--session` would bring the
    /// installed service files, so that a call to an unowned name could start
    /// a real program, and a directory in the live runtime directory.
    const BUS_CONFIG: &str = r#"<busconfig>
  <type>session</type>
  <listen>unix:tmpdir=/tmp</listen>
  <auth>EXTERNAL</auth>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>
"#;

    impl PrivateBus {
        /// `None` where there is no `dbus-daemon` to run. One that does not
        /// come up is a failure, not a skip: the tests would pass by doing
        /// nothing.
        fn start(name: &str) -> Option<Self> {
            use std::io::BufRead;
            let dir = std::env::temp_dir().join(format!(
                "minkashell-tray-test-{}-{name}",
                std::process::id()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            let config = dir.join("bus.conf");
            std::fs::write(&config, BUS_CONFIG).unwrap();
            let spawned = std::process::Command::new("dbus-daemon")
                .arg(format!("--config-file={}", config.display()))
                // Over the config's <listen>, which has to be there.
                .arg(format!("--address=unix:path={}", dir.join("bus").display()))
                .args(["--nofork", "--nopidfile", "--nosyslog", "--print-address=1"])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .spawn();
            let Ok(daemon) = spawned else {
                let _ = std::fs::remove_dir_all(&dir);
                return None;
            };
            // From here Drop stops the daemon and removes the directory.
            let mut bus = Self {
                daemon,
                dir,
                address: String::new(),
            };
            let stdout = bus.daemon.stdout.take().unwrap();
            let mut address = String::new();
            std::io::BufReader::new(stdout)
                .read_line(&mut address)
                .unwrap();
            assert!(
                !address.trim().is_empty(),
                "dbus-daemon did not start with the test's config"
            );
            bus.address = address.trim().to_owned();
            Some(bus)
        }

        async fn connect(&self) -> Connection {
            zbus::connection::Builder::address(self.address.as_str())
                .unwrap()
                .method_timeout(Duration::from_secs(5))
                .build()
                .await
                .unwrap()
        }
    }

    impl Drop for PrivateBus {
        fn drop(&mut self) {
            let _ = self.daemon.kill();
            let _ = self.daemon.wait();
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    /// Somebody else's watcher, with kded's manners: it takes a registration
    /// from anyone, supplies the path itself and answers the same whether or
    /// not it kept the item.
    struct Watcher {
        items: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
        keeps: bool,
    }

    #[zbus::interface(name = "org.kde.StatusNotifierWatcher")]
    impl Watcher {
        async fn register_status_notifier_item(&self, service: &str) {
            if self.keeps {
                self.items
                    .lock()
                    .unwrap()
                    .push(format!("{service}{ITEM_PATH}"));
            }
        }

        #[zbus(property)]
        async fn registered_status_notifier_items(&self) -> Vec<String> {
            self.items.lock().unwrap().clone()
        }
    }

    struct Item(&'static str);

    #[zbus::interface(name = "org.kde.StatusNotifierItem")]
    impl Item {
        #[zbus(property)]
        async fn id(&self) -> String {
            self.0.to_owned()
        }
    }

    /// A program with a tray item on the bus: its connection, kept open, and
    /// its unique name.
    async fn program(bus: &PrivateBus, id: &'static str) -> (Connection, String) {
        let connection = bus.connect().await;
        connection
            .object_server()
            .at(ITEM_PATH, Item(id))
            .await
            .unwrap();
        let name = connection.unique_name().unwrap().to_string();
        (connection, name)
    }

    async fn watcher(
        bus: &PrivateBus,
        keeps: bool,
    ) -> (Connection, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
        let items = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let connection = bus.connect().await;
        let object = Watcher {
            items: std::sync::Arc::clone(&items),
            keeps,
        };
        connection
            .object_server()
            .at(WATCHER_PATH, object)
            .await
            .unwrap();
        connection.request_name(WATCHER).await.unwrap();
        (connection, items)
    }

    /// The whole of a rescue against a watcher that is not this shell's.
    /// (The watcher is in this process too, so "this shell" is given as a
    /// process id that is not.)
    #[test]
    fn registers_what_a_foreign_watcher_lacks() {
        let Some(bus) = PrivateBus::start("rescue") else {
            return;
        };
        zbus::block_on(async {
            let (_watcher, items) = watcher(&bus, true).await;
            let (_orphan, orphan) = program(&bus, "Orphan").await;
            let (_known, known) = program(&bus, "Known").await;
            let (_changed, changed) = program(&bus, "Another item now").await;
            // A program that answers, but has nothing at the item's path.
            let plain = bus.connect().await;
            plain.object_server().at("/Other", Item("x")).await.unwrap();
            // The watcher already has one of them, under its own name.
            items.lock().unwrap().push(format!("{known}{ITEM_PATH}"));
            let shell = bus.connect().await;
            let pending = saved(&[
                (&orphan, "Orphan"),
                (&known, "Known"),
                (&changed, "What it used to be"),
                // A bus name nothing has: its program has exited.
                (":1.4000", "Exited"),
                (plain.unique_name().unwrap().as_str(), "No item"),
            ]);

            // Everything is settled in one pass: only the orphan is
            // registered, and it once.
            assert_eq!(pass(&shell, &pending, 0).await, Pass::Rest(Saved::new()));
            assert_eq!(
                *items.lock().unwrap(),
                [
                    format!("{known}{ITEM_PATH}"),
                    format!("{orphan}{ITEM_PATH}")
                ]
            );
            assert_eq!(pass(&shell, &pending, 0).await, Pass::Rest(Saved::new()));
            assert_eq!(items.lock().unwrap().len(), 2);

            // Under its own watcher the shell has nothing to do.
            assert_eq!(
                pass(&shell, &pending, std::process::id()).await,
                Pass::Nothing
            );
        });
    }

    /// A watcher's reply does not say whether it took the item: one that did
    /// not is found out from its list, and the item is kept for another try.
    #[test]
    fn a_watcher_that_ignores_the_call_is_found_out() {
        let Some(bus) = PrivateBus::start("ignored") else {
            return;
        };
        zbus::block_on(async {
            let (_watcher, items) = watcher(&bus, false).await;
            let (_orphan, orphan) = program(&bus, "Orphan").await;
            let shell = bus.connect().await;
            let pending = saved(&[(&orphan, "Orphan")]);
            assert_eq!(pass(&shell, &pending, 0).await, Pass::Rest(pending.clone()));
            assert!(items.lock().unwrap().is_empty());
        });
    }

    /// With no watcher on the bus there is nobody to tell: the shell is about
    /// to become it, and programs register with a new watcher themselves.
    #[test]
    fn no_watcher_means_nothing_to_do() {
        let Some(bus) = PrivateBus::start("none") else {
            return;
        };
        zbus::block_on(async {
            let (_orphan, orphan) = program(&bus, "Orphan").await;
            let shell = bus.connect().await;
            assert_eq!(
                pass(&shell, &saved(&[(&orphan, "Orphan")]), 0).await,
                Pass::Nothing
            );
        });
    }

    /// A watcher that has the name but no object behind it (kded6 while it
    /// starts, or with its module unloaded) gives no list: nothing is asked,
    /// and the pass says why.
    #[test]
    fn a_watcher_without_its_object_is_not_asked() {
        let Some(bus) = PrivateBus::start("unread") else {
            return;
        };
        zbus::block_on(async {
            let holder = bus.connect().await;
            // Something on the connection, so it answers for what it lacks.
            holder
                .object_server()
                .at("/Other", Item("x"))
                .await
                .unwrap();
            holder.request_name(WATCHER).await.unwrap();
            let (_orphan, orphan) = program(&bus, "Orphan").await;
            let shell = bus.connect().await;
            let outcome = pass(&shell, &saved(&[(&orphan, "Orphan")]), 0).await;
            assert!(matches!(outcome, Pass::Unread(_)), "{outcome:?}");
        });
    }

    #[test]
    fn join_all_keeps_order() {
        let futures: Vec<Pin<Box<dyn Future<Output = u32>>>> = vec![
            Box::pin(async { 1 }),
            Box::pin(async { 2 }),
            Box::pin(async { 3 }),
        ];
        assert_eq!(zbus::block_on(join_all(futures)), vec![1, 2, 3]);
        let none: Vec<Pin<Box<dyn Future<Output = u32>>>> = Vec::new();
        assert!(zbus::block_on(join_all(none)).is_empty());
    }
}
