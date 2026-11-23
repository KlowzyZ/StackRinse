"""Linux WebKitGTK smoke test for the built Tauri application."""
import argparse
import base64
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen


def webdriver(method, endpoint, payload=None):
    data = None if payload is None else json.dumps(payload).encode()
    request = Request(
        "http://127.0.0.1:4444" + endpoint, data=data, method=method,
        headers={"Content-Type": "application/json"},
    )
    try:
        with urlopen(request, timeout=30) as response:
            return json.load(response)["value"]
    except HTTPError as exc:
        raise RuntimeError(exc.read().decode()) from exc


def wait_for(condition, description):
    deadline = time.monotonic() + 30
    while time.monotonic() < deadline:
        if condition():
            return
        time.sleep(0.2)
    raise TimeoutError(description)


def driver_ready():
    try:
        return webdriver("GET", "/status")["ready"]
    except URLError:
        return False


def smoke(binary, screenshot):
    with tempfile.TemporaryDirectory(prefix="stackrinse-native-") as tmp:
        root = Path(tmp) / "workspace"
        project = root / "sample-crate"
        output = project / "target"
        output.mkdir(parents=True)
        (project / "Cargo.toml").write_text('[package]\nname = "sample-crate"\n')
        marker = output / ".rustc_info.json"
        marker.write_bytes(b"{}")
        config = Path(tmp) / "config"
        settings = config / "dev.klowzy.stackrinse" / "settings.json"
        settings.parent.mkdir(parents=True)
        settings.write_text(json.dumps({"roots": [str(root)]}))
        env = dict(os.environ, XDG_CONFIG_HOME=str(config), TAURI_WEBVIEW_AUTOMATION="true")
        driver = subprocess.Popen(["WebKitWebDriver", "--host=127.0.0.1", "--port=4444"], env=env)
        try:
            wait_for(driver_ready, "WebKitWebDriver did not become ready")
            capabilities = {"webkitgtk:browserOptions": {"binary": str(binary.resolve())}}
            session = webdriver("POST", "/session", {"capabilities": {"alwaysMatch": capabilities}})
            endpoint = "/session/" + session["sessionId"]
            def js(script):
                return webdriver("POST", endpoint + "/execute/sync", {"script": script, "args": []})

            try:
                wait_for(lambda: js('return !!document.querySelector("button.primary:enabled")'),
                         "Desktop scan control did not appear")
                js('document.querySelector("button.primary").click()')
                wait_for(lambda: js('return document.body.innerText.includes("Inventory measured")'),
                         "Native scan did not complete")
                assert js('return document.querySelector(".bigNumber").textContent.trim()') == "2 B"
                screenshot.write_bytes(base64.b64decode(webdriver("GET", endpoint + "/screenshot")))
                js('document.querySelector(".projectRow").click()')
                js('document.querySelector(".candidateBottom button").click()')
                js('document.querySelector(".sectionHead .linkBtn").click()')
                wait_for(lambda: js('return document.querySelector(".planTotal strong")?.textContent === "2 B"'),
                         "Native dry run did not return the selected directory")
                assert marker.read_bytes() == b"{}"
                assert js('return document.querySelector(".reviewFoot button:last-child").disabled')
            finally:
                webdriver("DELETE", endpoint)
        finally:
            driver.terminate()
            driver.wait(timeout=10)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("screenshot", type=Path)
    args = parser.parse_args()
    smoke(args.binary, args.screenshot)
