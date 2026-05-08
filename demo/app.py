"""Flaky Flask service for Glass Slipper network-debug demo.

Returns 503 every 3rd request. The agent should detect the pattern.
"""

from flask import Flask
import threading

app = Flask(__name__)
counter_lock = threading.Lock()
request_counter = 0


@app.route("/")
@app.route("/health")
def index():
    global request_counter
    with counter_lock:
        request_counter += 1
        count = request_counter

    if count % 3 == 0:
        return "Service temporarily unavailable", 503

    return f"OK (request #{count})", 200


if __name__ == "__main__":
    app.run(host="0.0.0.0", port=5000)
