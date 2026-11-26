"""Linux WebKitGTK smoke test for embedded inventory, history and dry-run review."""
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
        env = dict(os.environ, XDG_CONFIG_HOME=str(config), XDG_DATA_HOME=str(Path(tmp) / "data"),
                   TAURI_WEBVIEW_AUTOMATION="true")
        driver = subprocess.Popen(["WebKitWebDriver", "--host=127.0.0.1", "--port=4444"], env=env)
        try:
            wait_for(driver_ready, "WebKitWebDriver did not become ready")
            capabilities = {"webkitgtk:browserOptions": {"binary": str(binary.resolve())}}
            def open_session():
                session = webdriver("POST", "/session", {"capabilities": {"alwaysMatch": capabilities}})
                return "/session/" + session["sessionId"]

            endpoint = open_session()

            def js(script):
                return webdriver("POST", endpoint + "/execute/sync", {"script": script, "args": []})

            def command(command, arguments, expect_error=False):
                script = """
                    const [command, args, done] = arguments;
                    window.__TAURI_INTERNALS__.invoke(command, args)
                        .then(result => done({result}), error => done({error: String(error)}));
                """
                response = webdriver("POST", endpoint + "/execute/async", {
                    "script": script, "args": [command, arguments],
                })
                if expect_error:
                    return response["error"]
                assert "error" not in response, response
                return response.get("result")

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
                assert js('return document.querySelector(".reviewActions button").textContent.includes("Remove from review")')
                js('document.querySelector(".reviewActions button").click()')
                wait_for(lambda: js('return document.body.innerText.includes("Your review is empty")'),
                         "Removing a preview entry did not clear the selection")
                assert marker.read_bytes() == b"{}"
                assert js('return document.querySelector(".reviewFoot button:last-child").disabled')

                assert len(command("history_list", {})["entries"]) == 1
                markers = (Path(__file__).resolve().parents[3] /
                           "crates/stackrinse-core/tests/fixtures/ecosystem-markers")
                dart_project = root / "pub-client"
                dart_tool = dart_project / ".dart_tool"
                dart_tool.mkdir(parents=True)
                (dart_project / "pubspec.yaml").write_text("name: storage_sample\n")
                dart_bytes = 0
                for name in ["package_config.json", "package_graph.json"]:
                    dart_bytes += (dart_tool / name).write_bytes((markers / name).read_bytes())
                (output / "compiled").write_bytes(b"A" * 1048576)
                report = command("scan", {})
                assert report["totals"]["logical_bytes"] == 1048578 + dart_bytes
                dart = next(p for p in report["projects"] if p["name"] == "pub-client")
                candidate = dart["candidates"][0]
                assert candidate["ecosystem"] == "Dart" and candidate["classification"] == "review_required"
                (output / "compiled").write_bytes(b"A" * 524288)
                assert command("scan", {})["totals"]["logical_bytes"] == 524290 + dart_bytes
                entries = command("history_list", {})["entries"]
                assert len(entries) == 3 and all(entry["complete"] for entry in entries)
                assert all(entry["discovery_rule_version"] == report["discovery_rule_version"] for entry in entries)
                detail = command("history_detail", {"id": entries[0]["id"]})
                assert detail["ecosystems"]["Dart"] == dart_bytes
                result = command("history_compare", {"beforeId": entries[1]["id"], "afterId": entries[0]["id"]})
                assert result["logical_bytes_delta"] == -524288
                js('document.querySelector(\'nav button[title="History"]\').click()')
                wait_for(lambda: js('return document.body.innerText.includes("3 comparable scans")'),
                         "Native storage trend did not render")
                wait_for(lambda: js('return document.querySelector(".comparisonTotal strong")?.textContent === "−512 KiB"'),
                         "Native comparison did not render")
                screenshot.with_name("native-history.png").write_bytes(
                    base64.b64decode(webdriver("GET", endpoint + "/screenshot")))

                # Keep settings and selection identical, then make the replacement scan fail.
                moved = root.rename(root.with_name("moved"))
                assert "Cannot scan" in command("scan", {}, expect_error=True)
                assert "Scan a folder first" in command("dry_run", {"paths": [str(output)]}, expect_error=True)
                moved.rename(root)
                assert len(command("history_list", {})["entries"]) == 3
                command("history_set_retention", {"count": 10})
            finally:
                webdriver("DELETE", endpoint)

            endpoint = open_session()
            try:
                wait_for(lambda: js('return document.body.innerText.includes("Latest saved scan")'),
                         "Saved history did not appear after restarting the native application")
                assert js('return document.body.innerText.includes("Awaiting first scan")')
                history = command("history_list", {})
                assert history["retention"] == 10 and len(history["entries"]) == 3
                assert "Scan a folder first" in command("dry_run", {"paths": [str(output)]}, expect_error=True)
                command("history_remove", {"id": history["entries"][0]["id"]})
                assert len(command("history_list", {})["entries"]) == 2
                command("history_clear", {})
                assert not command("history_list", {})["entries"]
                assert marker.read_bytes() == b"{}"
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
