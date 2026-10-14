"""Validate adapter transport with a fixture or handshake-only with real Codex.
Never logs account details, starts a real login or sends a real inference.
"""
import argparse
import json
from pathlib import Path
import queue
import subprocess
import threading
import time


class Client:
    def __init__(self, adapter):
        self.process = subprocess.Popen([str(adapter)], stdin=subprocess.PIPE,
            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True,
            encoding="utf-8", creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
        self.messages = queue.Queue()
        self.backlog = []
        self.next_id = 0
        def read():
            for line in self.process.stdout:
                self.messages.put(json.loads(line))
            self.messages.put({"eof": True})
        threading.Thread(target=read, daemon=True).start()

    def wait(self, predicate):
        deadline = time.monotonic() + 35
        while True:
            for i, message in enumerate(self.backlog):
                if predicate(message):
                    return self.backlog.pop(i)
            message = self.messages.get(timeout=max(0.1, deadline-time.monotonic()))
            if message.get("eof"):
                raise AssertionError("Adapter stopped unexpectedly")
            if predicate(message):
                return message
            self.backlog.append(message)
            if time.monotonic() >= deadline:
                raise AssertionError("Adapter timeout")

    def call(self, method, params=None):
        self.next_id += 1
        request_id = self.next_id
        self.process.stdin.write(json.dumps({"jsonrpc":"2.0", "id":request_id,
            "method":method, "params":params or {}})+"\n")
        self.process.stdin.flush()
        return self.wait(lambda m: m.get("id") == request_id)

    def event(self, method):
        return self.wait(lambda m: m.get("method") == method)["params"]

    def close(self):
        if self.process.poll() is None:
            try:
                self.call("shutdown")
                self.process.wait(timeout=5)
            finally:
                if self.process.poll() is None:
                    self.process.kill()
        self.process.stdin.close()
        self.process.stdout.close()


def main():
    parser=argparse.ArgumentParser()
    parser.add_argument("adapter", type=Path)
    parser.add_argument("--provider", type=Path)
    parser.add_argument("--mock", action="store_true")
    parser.add_argument("--login-cancel", action="store_true", help="Start and cancel official browser login; do not sign in or print URLs")
    args=parser.parse_args()
    client=Client(args.adapter.resolve())
    try:
        initialized=client.call("initialize", {"protocolVersion":1,"clientVersion":"test",
            "providerPath":str(args.provider.resolve()) if args.provider else ""})
        assert initialized["result"]["protocolVersion"] == 1
        assert "account" in client.call("account/read")["result"]
        assert isinstance(client.call("models/list")["result"]["models"], list)
        scope = {"scopeToken":"test-scope", "sessions":[{"sessionId":1,"name":"Test local","type":"local","state":"connected"}]}
        if args.login_cancel:
            login=client.call('account/login')['result']
            assert login['authUrl'].startswith('https://auth.openai.com/')
            assert login['loginId']
            assert 'result' in client.call('account/login/cancel', {'loginId':login['loginId']})
            assert 'account' in client.call('account/read')['result']
        if args.mock:
            assert client.call("account/login")["result"]["loginId"] == "mock-login"
            assert client.event("account/updated")["account"]["type"] == "chatgpt"
            assert "result" in client.call("conversation/reset")
            started=client.call("chat/start", {"text":"test","scope":scope})
            assert started["result"]["conversationId"] == "mock-thread"
            tool=client.wait(lambda m: m.get('method')=='tools/call')
            assert tool['params']['scopeToken']=='test-scope'
            client.process.stdin.write(json.dumps({'jsonrpc':'2.0','id':tool['id'],'result':{'sessions':scope['sessions']}})+'\n');client.process.stdin.flush()
            assert client.event("chat/message")["text"] == "Hola mundo"
            assert client.event("chat/completed")["status"] == "completed"
            # Resume the saved provider thread with fresh authority after reconnect.
            client.close()
            client = Client(args.adapter.resolve())
            assert "result" in client.call("initialize", {"protocolVersion":1,"clientVersion":"test","providerPath":str(args.provider.resolve())})
            resumed_scope = {**scope, "scopeToken": "resumed-scope"}
            resumed = client.call("chat/start", {"conversationId":"mock-thread", "text":"interrupt", "scope":resumed_scope})
            assert resumed["result"]["conversationId"] == "mock-thread"
            assert "result" in client.call("chat/interrupt")
            assert client.event("chat/completed")["status"] == "interrupted"
            scope = resumed_scope
            assert "error" in client.call("chat/start", {"text":"fail","scope":scope})
            assert "result" in client.call("chat/start", {"text":"interrupt","scope":scope})
            assert "result" in client.call("chat/interrupt")
            assert client.event("chat/completed")["status"] == "interrupted"
            assert "result" in client.call("account/logout")
        print("PASS: fixture login, scoped tools, denied native tool, chat, thread resume after process restart, failure, interrupt and shutdown" if args.mock
              else "PASS: real Codex initialize, account/read, models/list and shutdown; no inference" + ("; official login start/cancel" if args.login_cancel else "; no login"))
    finally:
        client.close()


if __name__ == "__main__":
    main()
