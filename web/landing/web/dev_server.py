"""Server for viewing the site during development: python dev_server.py [port] — http://127.0.0.1:8821.

Like `python -m http.server`, but forbids caching: otherwise after edits the browser takes the old styles.css / site.js from the cache
(the page is at the new address, but the styles and scripts are the old ones). On the production hosting _headers does the same (no-cache).
"""
import functools
import http.server
import pathlib
import sys


class NoCache(http.server.SimpleHTTPRequestHandler):
    def end_headers(self):
        self.send_header("Cache-Control", "no-store")
        super().end_headers()


def main():
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8821
    handler = functools.partial(NoCache, directory=str(pathlib.Path(__file__).resolve().parent))
    with http.server.ThreadingHTTPServer(("127.0.0.1", port), handler) as srv:
        print(f"http://127.0.0.1:{port}/", flush=True)
        srv.serve_forever()


if __name__ == "__main__":
    main()
