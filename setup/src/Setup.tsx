import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open } from "@tauri-apps/plugin-dialog";
import { useEffect, useState, type ReactNode } from "react";
import { Icon } from "../../src/components/Icon";
import { Logo } from "../../src/components/Logo";

interface Info {
  version: string;
  uninstall: boolean;
  installed: { version: string; dir: string; compare: -1 | 0 | 1 } | null;
  defaultDir: string;
  license: string;
  hasPayload: boolean;
  appRunning: boolean;
}

type Job = "install" | "update" | "uninstall";
type Screen = "license" | "options" | "installed" | "confirm" | "work" | "done" | "error";

const STEPS: Record<"install" | "uninstall", [string, string][]> = {
  install: [
    ["extract", "Unpacking files…"],
    ["install", "Installing Lustro…"],
    ["options", "Applying your options…"],
    ["uninstaller", "Setting up the uninstaller…"],
  ],
  uninstall: [
    ["remove", "Removing Lustro…"],
    ["data", "Deleting scans and settings…"],
  ],
};

const win = getCurrentWindow();

function TitleBar() {
  return (
    <header className="titlebar" data-tauri-drag-region>
      <Logo />
      <span className="appname" data-tauri-drag-region>
        Lustro Setup
      </span>
      <span className="grow" data-tauri-drag-region />
      <div className="caps">
        <button aria-label="Minimize" onClick={() => win.minimize()}>
          <Icon name="min" />
        </button>
        <button className="x" aria-label="Close" onClick={() => win.close()}>
          <Icon name="x" />
        </button>
      </div>
    </header>
  );
}

function Hero({ title, sub }: { title: string; sub: ReactNode }) {
  return (
    <div className="su-hero">
      <Logo size={52} />
      <div>
        <h1>{title}</h1>
        <p>{sub}</p>
      </div>
    </div>
  );
}

export function Setup() {
  const [info, setInfo] = useState<Info | null>(null);
  const [screen, setScreen] = useState<Screen>("license");
  const [back, setBack] = useState<Screen>("license");
  const [dir, setDir] = useState("");
  const [desktop, setDesktop] = useState(true);
  const [startWithWindows, setStartWithWindows] = useState(false);
  const [explorerMenu, setExplorerMenu] = useState(false);
  const [deleteData, setDeleteData] = useState(false);
  const [job, setJob] = useState<Job>("install");
  const [step, setStep] = useState("");
  const [error, setError] = useState("");

  useEffect(() => {
    invoke<Info>("info").then((i) => {
      setInfo(i);
      setDir(i.defaultDir);
      setScreen(i.uninstall ? "confirm" : i.installed ? "installed" : "license");
    });
    const off = listen<string>("step", (e) => setStep(e.payload));
    return () => {
      off.then((f) => f());
    };
  }, []);

  if (!info) return <div className="app su" />;

  const run = async (j: Job) => {
    setBack(screen);
    setJob(j);
    setStep(j === "uninstall" ? "remove" : "extract");
    setScreen("work");
    try {
      if (j === "uninstall") await invoke("uninstall", { deleteData });
      else
        await invoke("install", {
          args: { dir: j === "update" ? info.installed!.dir : dir, desktop, startWithWindows, explorerMenu, update: j === "update" },
        });
      setScreen("done");
    } catch (e) {
      setError(String(e));
      setScreen("error");
    }
  };

  const pickDir = async () => {
    const picked = await open({ directory: true, defaultPath: dir }).catch(() => null);
    // Install into a "Lustro" folder inside the folder that the user picks.
    if (typeof picked === "string") setDir(/\\lustro\\?$/i.test(picked) ? picked : `${picked.replace(/\\+$/, "")}\\Lustro`);
  };

  const running = info.appRunning && (
    <div className="note warn">
      <Icon name="warn" />
      <span>Lustro is running. Setup will close it.</span>
    </div>
  );

  let body: ReactNode;
  switch (screen) {
    case "license":
      body = (
        <>
          <Hero title={`Install Lustro ${info.version}`} sub="Storage manager for Windows 11. Free and open source." />
          <div className="su-sec">License</div>
          <pre className="su-license selectable">{info.license}</pre>
          <footer className="su-ft">
            <span className="faint">Step 1 of 2</span>
            <button className="btn accent" onClick={() => setScreen("options")} autoFocus>
              Next
            </button>
          </footer>
        </>
      );
      break;

    case "options":
      body = (
        <>
          <Hero title="Install options" sub="You can change the Windows options later in Lustro Settings." />
          <div className="su-sec">Install to</div>
          <div className="row" style={{ flexWrap: "nowrap" }}>
            <input className="text mono" id="dir" value={dir} onChange={(e) => setDir(e.target.value)} spellCheck={false} style={{ flex: 1 }} />
            <button className="btn" onClick={pickDir}>
              <Icon name="folder" />
              Change
            </button>
          </div>
          <div className="su-checks">
            <label>
              <input type="checkbox" id="desktop" checked={desktop} onChange={(e) => setDesktop(e.target.checked)} />
              <span>Make a desktop shortcut</span>
            </label>
            <label>
              <input type="checkbox" id="startWin" checked={startWithWindows} onChange={(e) => setStartWithWindows(e.target.checked)} />
              <span>
                Start Lustro with Windows <em>Lustro starts in the tray and warns you about low space.</em>
              </span>
            </label>
            <label>
              <input type="checkbox" id="menu" checked={explorerMenu} onChange={(e) => setExplorerMenu(e.target.checked)} />
              <span>
                Add "Analyze with Lustro" to the Explorer menu <em>It shows under "Show more options" for folders and drives.</em>
              </span>
            </label>
          </div>
          {running}
          {!info.hasPayload && (
            <div className="note warn">
              <Icon name="warn" />
              <span>This is a dev build with no Lustro installer inside. Install will fail.</span>
            </div>
          )}
          <footer className="su-ft">
            <span className="faint">Step 2 of 2 · No admin rights needed</span>
            <div className="row">
              <button className="btn" onClick={() => setScreen("license")}>
                Back
              </button>
              <button className="btn accent" disabled={!dir.trim()} onClick={() => run("install")} autoFocus>
                Install
              </button>
            </div>
          </footer>
        </>
      );
      break;

    case "installed": {
      const inst = info.installed!;
      const newer = inst.compare === 1;
      body = (
        <>
          <Hero
            title={newer ? "A newer Lustro is installed" : `Lustro ${inst.version} is installed`}
            sub={newer ? `Lustro ${inst.version} is installed. This setup has ${info.version}.` : `This setup has Lustro ${info.version}.`}
          />
          <div className="su-sec">Installed in</div>
          <div className="text mono su-path">{inst.dir}</div>
          {running}
          <footer className="su-ft">
            <button className="btn ghost danger" onClick={() => setScreen("confirm")}>
              <Icon name="trash" />
              Uninstall
            </button>
            {newer ? (
              <button className="btn accent" onClick={() => win.close()} autoFocus>
                Close
              </button>
            ) : (
              <button className="btn accent" onClick={() => run("update")} autoFocus>
                {inst.compare === 0 ? "Repair" : `Update to ${info.version}`}
              </button>
            )}
          </footer>
        </>
      );
      break;
    }

    case "confirm":
      body = (
        <>
          <Hero title="Remove Lustro?" sub="Setup removes the app, its shortcuts and its Windows menu entries." />
          <div className="su-checks">
            <label>
              <input type="checkbox" id="deleteData" checked={deleteData} onChange={(e) => setDeleteData(e.target.checked)} />
              <span>
                Also delete scans, history and settings <em>Without this, Lustro keeps them for a later install.</em>
              </span>
            </label>
          </div>
          {running}
          <footer className="su-ft">
            <span />
            <div className="row">
              <button className="btn" onClick={() => (info.uninstall ? win.close() : setScreen("installed"))} autoFocus>
                Cancel
              </button>
              <button className="btn danger-fill" onClick={() => run("uninstall")}>
                Uninstall
              </button>
            </div>
          </footer>
        </>
      );
      break;

    case "work": {
      const list = STEPS[job === "uninstall" ? "uninstall" : "install"];
      const i = Math.max(0, list.findIndex(([k]) => k === step));
      return (
        <div className="app su">
          <TitleBar />
          <main className="su-body su-center" aria-live="polite">
            <Logo size={56} />
            <h1>{job === "uninstall" ? "Removing Lustro" : job === "update" ? "Updating Lustro" : "Installing Lustro"}</h1>
            <div className="su-bar" role="progressbar" aria-valuemin={0} aria-valuemax={list.length} aria-valuenow={i + 1}>
              <i style={{ width: `${((i + 0.6) / list.length) * 100}%` }} />
            </div>
            <span className="faint">{list[i][1]}</span>
          </main>
        </div>
      );
    }

    case "done":
      body =
        job === "uninstall" ? (
          <div className="su-center">
            <span className="su-ok" />
            <h1>Lustro is removed</h1>
            <p className="muted">{deleteData ? "Scans, history and settings are deleted too." : "Your scans and settings stay for a later install."}</p>
            <button className="btn accent" onClick={() => win.close()} autoFocus>
              Close
            </button>
          </div>
        ) : (
          <div className="su-center">
            <span className="su-ok" />
            <h1>{job === "update" ? `Lustro ${info.version} is ready` : "Lustro is ready"}</h1>
            <p className="muted">You can find it in the Start menu.</p>
            <div className="row">
              <button className="btn" onClick={() => win.close()}>
                Close
              </button>
              <button
                className="btn accent"
                autoFocus
                onClick={() =>
                  invoke("launch")
                    .then(() => win.close())
                    .catch((e) => {
                      setError(String(e));
                      setScreen("error");
                    })
                }
              >
                Open Lustro
              </button>
            </div>
          </div>
        );
      break;

    case "error":
      body = (
        <div className="su-center">
          <span className="su-err">
            <Icon name="warn" />
          </span>
          <h1>{job === "uninstall" ? "Lustro was not removed" : "Lustro was not installed"}</h1>
          <p className="muted selectable su-msg">{error}</p>
          <div className="row">
            <button className="btn" onClick={() => win.close()}>
              Close
            </button>
            <button className="btn accent" onClick={() => setScreen(back)} autoFocus>
              Try again
            </button>
          </div>
        </div>
      );
      break;
  }

  return (
    <div className="app su">
      <TitleBar />
      <main className="su-body">{body}</main>
    </div>
  );
}
