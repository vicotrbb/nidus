#!/usr/bin/env python3
"""Disposable loopback collector for example OTLP and Sentry flush verification."""
import http.server
import pathlib
import sys

port_file, events_file = map(pathlib.Path, sys.argv[1:])


class Receiver(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        payload = self.rfile.read(int(self.headers.get("Content-Length", "0")))
        with events_file.open("a") as events:
            events.write(f"{self.path} {len(payload)}\n")
        self.send_response(200)
        self.send_header("Content-Length", "0")
        self.end_headers()

    def log_message(self, *args):
        pass


with http.server.HTTPServer(("127.0.0.1", 0), Receiver) as server:
    port_file.write_text(str(server.server_port))
    server.serve_forever()
