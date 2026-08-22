"""Minimal stand-in for a Beam media server.

Beam is a separate self-hosted product, so the streaming integration is
otherwise impossible to exercise locally. This implements just the three
endpoints core/src/streaming/beam/client.rs calls, backed by a fixed library,
so `just up` can demonstrate library sync, availability, and deep links.

Development only — it accepts any bearer token. Started by the `dev-beam`
compose profile; never enabled by default.
"""

import json
import os
from http.server import BaseHTTPRequestHandler, HTTPServer
from urllib.parse import urlparse

# provider_ids are what the matcher joins on to reach a canonical Stacks id.
LIBRARY = [
    {
        "id": "beam-dune-2021",
        "title": "Dune",
        "year": 2021,
        "media_type": "movie",
        "provider_ids": {"tmdb": 438631},
    },
    {
        "id": "beam-breaking-bad",
        "title": "Breaking Bad",
        "year": 2008,
        "media_type": "tv",
        "provider_ids": {"tmdb": 1396},
    },
    {
        "id": "beam-fight-club",
        "title": "Fight Club",
        "year": 1999,
        "media_type": "movie",
        "provider_ids": {"tmdb": 550},
    },
]

PUBLIC_URL = os.environ.get("MOCK_BEAM_PUBLIC_URL", "http://localhost:9494")


class Handler(BaseHTTPRequestHandler):
    def _send(self, payload, status=200):
        body = json.dumps(payload).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        path = urlparse(self.path).path
        if path == "/api/health":
            return self._send({"version": "mock-0.1.0"})
        if path == "/api/library":
            return self._send(LIBRARY)
        if path.startswith("/api/library/") and path.endswith("/playback"):
            item = path[len("/api/library/") : -len("/playback")]
            if not any(i["id"] == item for i in LIBRARY):
                return self._send({"error": "not found"}, 404)
            return self._send({"url": f"{PUBLIC_URL}/stream/{item}.m3u8"})
        return self._send({"error": "not found"}, 404)

    def log_message(self, fmt, *args):
        print("mock-beam:", fmt % args, flush=True)


if __name__ == "__main__":
    port = int(os.environ.get("MOCK_BEAM_PORT", "9494"))
    print(f"mock-beam listening on :{port}", flush=True)
    HTTPServer(("0.0.0.0", port), Handler).serve_forever()
