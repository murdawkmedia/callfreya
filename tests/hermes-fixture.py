"""Minimal loopback fixture for the Hermes chat completions contract."""
import json
import sys
from http.server import BaseHTTPRequestHandler, HTTPServer


class Gateway(BaseHTTPRequestHandler):
    def do_POST(self):
        if self.path != "/v1/chat/completions" or self.headers.get("Authorization") != "Bearer synthetic-key":
            self.send_error(401)
            return
        payload = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        if payload["stream"] or "Harbor Desk" not in payload["messages"][-1]["content"]:
            self.send_error(400)
            return
        body = json.dumps({"choices": [{"message": {"content": "Synthetic demo follow-up."}}]}).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


if __name__ == "__main__":
    HTTPServer(("127.0.0.1", int(sys.argv[1])), Gateway).serve_forever()
