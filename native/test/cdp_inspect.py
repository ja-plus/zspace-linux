#!/usr/bin/env python3
"""Inspect the native app renderer via CDP: DOM state, console errors, screenshot."""
import asyncio, json, base64, urllib.request
import websockets

def get_ws():
    targets = json.load(urllib.request.urlopen('http://127.0.0.1:9222/json'))
    for t in targets:
        if t.get('type') == 'page':
            return t['webSocketDebuggerUrl']
    raise SystemExit('no page target')

async def main():
    ws_url = get_ws()
    async with websockets.connect(ws_url, max_size=80*1024*1024) as ws:
        mid = 0
        errors = []
        async def cmd(method, params=None):
            nonlocal mid
            mid += 1
            await ws.send(json.dumps({"id": mid, "method": method, "params": params or {}}))
            while True:
                msg = json.loads(await ws.recv())
                if msg.get("method") == "Runtime.exceptionThrown":
                    errors.append(msg["params"]["exceptionDetails"].get("text", "") + " " +
                                  str(msg["params"]["exceptionDetails"].get("exception", {}).get("description", ""))[:200])
                if msg.get("method") == "Runtime.consoleAPICalled" and msg["params"]["type"] == "error":
                    errors.append("console.error: " + " ".join(str(a.get("value", a.get("description", "")))[:150] for a in msg["params"]["args"]))
                if msg.get("id") == mid:
                    return msg
        await cmd("Runtime.enable")
        await cmd("Page.enable")
        expr = """JSON.stringify({
            title: document.title,
            url: location.href,
            bodyLen: document.body ? document.body.innerHTML.length : -1,
            appLen: document.getElementById('app') ? document.getElementById('app').innerHTML.length : -1,
            inputs: document.querySelectorAll('input').length,
            buttons: document.querySelectorAll('button').length,
            bodyText: document.body ? document.body.innerText.slice(0,200) : ''
        })"""
        r = await cmd("Runtime.evaluate", {"expression": expr, "returnByValue": True})
        print("DOM:", r.get("result", {}).get("result", {}).get("value"))
        # let pending console events drain
        try:
            while True:
                msg = json.loads(await asyncio.wait_for(ws.recv(), timeout=1.0))
                if msg.get("method") == "Runtime.exceptionThrown":
                    errors.append(str(msg["params"]["exceptionDetails"])[:200])
        except asyncio.TimeoutError:
            pass
        print("ERRORS:", errors[:10] if errors else "none")
        s = await cmd("Page.captureScreenshot", {"format": "png"})
        data = s.get("result", {}).get("data")
        if data:
            open("/tmp/native-cdp.png", "wb").write(base64.b64decode(data))
            print("screenshot: /tmp/native-cdp.png")

asyncio.run(main())