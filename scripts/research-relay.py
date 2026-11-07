"""Fetch bounded HTTPS documents for an Agel byte stream on loopback."""

import argparse
import socketserver
from urllib.parse import urlsplit
from urllib.request import HTTPRedirectHandler, ProxyHandler, Request, build_opener


class NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


class ResearchRequest(socketserver.StreamRequestHandler):
    def handle(self):
        self.connection.settimeout(10)
        try:
            line = self.rfile.readline(2050)
            if not line.endswith(b"\n") or len(line) > 2049:
                raise ValueError("invalid request")
            url = line[:-1].decode("ascii")
            parts = urlsplit(url)
            if (parts.scheme != "https" or parts.netloc not in self.server.origins
                    or parts.fragment or any(ord(c) <= 32 or ord(c) >= 127 for c in url)):
                raise ValueError("origin refused")
            request = Request(url, headers={"User-Agent": "Agel research", "Accept": "text/html, text/plain"})
            with build_opener(ProxyHandler({}), NoRedirect()).open(request, timeout=10) as response:
                if response.status != 200:
                    raise ValueError("status refused")
                kind = {"text/html": b"html\n", "text/plain": b"text\n"}.get(response.headers.get_content_type())
                if not kind:
                    raise ValueError("media type refused")
                body = response.read(32764)
                if len(body) > 32763:
                    raise ValueError("response too large")
                body.decode("utf-8")
            self.wfile.write(kind + body)
        except (OSError, ValueError):
            self.wfile.write(b"deny\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", type=int, default=0)
    parser.add_argument("--host", action="append", required=True, help="HTTPS hostname to admit")
    args = parser.parse_args()
    if any(not host or any(not (c.isascii() and (c.isalnum() or c in ".-")) for c in host)
           for host in args.host):
        parser.error("--host requires an ASCII DNS hostname")
    with socketserver.TCPServer(("127.0.0.1", args.port), ResearchRequest) as server:
        server.origins = frozenset(args.host)
        print(server.server_address[1], flush=True)
        server.serve_forever()


if __name__ == "__main__":
    main()
