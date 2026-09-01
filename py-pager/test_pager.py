#!/usr/bin/env python3
"""py-pager 协议级测试：假宿主（同样只按文档手写）驱动完整生命周期。

覆盖：连接重试、hit→claim+layer.open（含 :line:col 剥离与 cwd 相对解析）、
layer.ready→layer.html、不可认领→hit.ignore、layer.close 清理、未知 type
忽略、版本门（v!=0 → 退出码 78）、EOF 正常退出。

用法：python3 py-pager/test_pager.py（自动找同目录 py-pager）
"""

import json
import os
import select
import socket
import struct
import subprocess
import sys
import tempfile
import threading
import time

HERE = os.path.dirname(os.path.abspath(__file__))
PAGER = os.path.join(HERE, "py-pager")
V = 0

PASS = 0
FAIL = 0


def ok(name):
    global PASS
    PASS += 1
    print(f"  [PASS] {name}")


def bad(name, detail=""):
    global FAIL
    FAIL += 1
    print(f"  [FAIL] {name} {detail}")


def send(sock, obj):
    data = json.dumps(obj, ensure_ascii=False, separators=(",", ":")).encode()
    sock.sendall(struct.pack("<I", len(data)) + data)


def recv_msg(sock, timeout=3.0, _bufs={}):
    """收一帧 → dict；超时/断开 → None。缓冲按 socket 保留
    （一次 recv 可能拿到多帧，丢弃余字节会丢消息）。"""
    buf = _bufs.setdefault(sock.fileno(), bytearray())
    deadline = time.monotonic() + timeout
    while True:
        if len(buf) >= 4:
            (n,) = struct.unpack("<I", buf[:4])
            if len(buf) - 4 >= n:
                payload = bytes(buf[4 : 4 + n])
                del buf[: 4 + n]
                return json.loads(payload.decode())
        remain = deadline - time.monotonic()
        if remain <= 0:
            return None
        r, _, _ = select.select([sock], [], [], remain)
        if not r:
            return None
        d = sock.recv(65536)
        if not d:
            return None
        buf += d


class FakeHost:
    """按文档手写的宿主半边：bind → 拉起 pager → 收发帧。"""

    def __init__(self, tmpdir):
        self.sock_path = os.path.join(tmpdir, "ade.sock")
        try:
            os.unlink(self.sock_path)
        except FileNotFoundError:
            pass
        self.srv = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.srv.bind(self.sock_path)
        self.srv.listen(1)
        self.srv.settimeout(5)
        self.proc = subprocess.Popen(
            [sys.executable, PAGER],
            env=dict(os.environ, NINJA_ADE_SOCK=self.sock_path),
            stderr=subprocess.PIPE,
        )

    def accept(self):
        self.conn = self.srv.accept()[0]
        return self.conn

    def wait_exit(self, timeout=5):
        try:
            return self.proc.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            self.proc.kill()
            return None


def spawn_host(tmpdir):
    h = FakeHost(tmpdir)
    try:
        h.accept()
        return h
    except socket.timeout:
        bad("pager 连接（5s 内）", h.proc.stderr.read().decode())
        h.proc.kill()
        return None


def hit_of(mid, text, cwd="", kind="path"):
    return {
        "type": "hit",
        "v": V,
        "id": mid,
        "kind": kind,
        "text": text,
        "cwd": cwd,
        "row": 3,
        "col": 0,
        "pane": 1,
        "modifiers": ["cmd"],
    }


def main():
    tmp = tempfile.mkdtemp(prefix="pypager-test-")
    sample = os.path.join(tmp, "sample.txt")
    with open(sample, "w") as f:
        f.write("alpha\nbeta <tag> & \"quote\"\ngamma\n")
    binary = os.path.join(tmp, "blob.bin")
    with open(binary, "wb") as f:
        f.write(b"\x00\x01binary")
    rel = os.path.join(tmp, "rel.md")
    with open(rel, "w") as f:
        f.write("# hi\n")

    # ---- 全周期：claim → open → ready → html ------------------------------
    h = spawn_host(tmp)
    if h:
        send(h.conn, hit_of(7, f"{sample}:2"))
        m = recv_msg(h.conn)
        if m == {"type": "hit.claim", "v": V, "id": 7, "priority": 50}:
            ok("hit.claim（id/priority）")
        else:
            bad("hit.claim", repr(m))
        m = recv_msg(h.conn)
        if (
            m
            and m.get("type") == "layer.open"
            and m.get("id") == 7
            and m.get("placement") == "tab"
            and m.get("surface") == "html"
        ):
            ok("layer.open（tab × html，id=hit.id）")
        else:
            bad("layer.open", repr(m))
        send(
            h.conn,
            {
                "type": "layer.ready",
                "v": V,
                "id": 7,
                "layer": 42,
                "width_px": 800,
                "height_px": 600,
                "dpi": 144,
                "io_surface_id": 0,
            },
        )
        m = recv_msg(h.conn)
        if (
            m
            and m.get("type") == "layer.html"
            and m.get("layer") == 42
            and "beta &lt;tag&gt; &amp; &quot;quote&quot;" in m.get("html", "")
            and 'class="cur"' in m.get("html", "")
        ):
            ok("layer.html（转义内容 + :2 行高亮）")
        else:
            bad("layer.html", repr(m)[:200])

        # 相对路径（cwd 解析）+ 未知 type 忽略 + layer.close 清理
        send(h.conn, {"type": "config.push", "v": V, "enabled": ["x"], "keys": {}, "memory_limit_bytes": 1})
        send(h.conn, hit_of(8, "rel.md", cwd=f"file://{tmp}"))
        m = recv_msg(h.conn)
        if m and m.get("type") == "hit.claim" and m.get("id") == 8:
            ok("相对路径经 cwd 认领（file:// 剥离）")
        else:
            bad("相对路径认领", repr(m))
        recv_msg(h.conn)  # layer.open（不细验）
        send(h.conn, {"type": "layer.ready", "v": V, "id": 8, "layer": 43, "width_px": 1, "height_px": 1, "dpi": 72, "io_surface_id": 0})
        m = recv_msg(h.conn)
        if m and m.get("type") == "layer.html" and "# hi" in m.get("html", ""):
            ok("相对路径渲染")
        else:
            bad("相对路径渲染", repr(m)[:120])
        send(h.conn, {"type": "layer.close", "v": V, "layer": 42})
        m = recv_msg(h.conn)
        if m is None:
            ok("layer.close 后静默（无回帧）")
        else:
            bad("layer.close 后应无回帧", repr(m))

        # 不可认领：二进制 / 不存在 / 非 path
        for i, (mid, text, cwd, kind) in enumerate(
            [
                (20, binary, "", "path"),
                (21, os.path.join(tmp, "nope.txt"), "", "path"),
                (22, "https://example.com/x", "", "url"),
            ]
        ):
            send(h.conn, hit_of(mid, text, cwd, kind))
            m = recv_msg(h.conn)
            if m == {"type": "hit.ignore", "v": V, "id": mid}:
                ok(f"hit.ignore（{['二进制', '不存在', 'url kind'][i]}）")
            else:
                bad(f"hit.ignore #{i}", repr(m))

        # EOF → 退出 0
        h.conn.close()
        code = h.wait_exit()
        if code == 0:
            ok("EOF → 退出码 0")
        else:
            bad("EOF 退出码", repr(code))
        h.srv.close()

    # ---- 版本门：v != 0 → 退出 78 ----------------------------------------
    h2 = spawn_host(tmp)
    if h2:
        send(h2.conn, {"type": "hit", "v": 9, "id": 1})
        code = h2.wait_exit()
        if code == 78:
            ok("版本门 v!=0 → 退出码 78")
        else:
            bad("版本门退出码", repr(code))
        h2.srv.close()

    print(f"\n== py-pager 协议测试：PASS {PASS} / FAIL {FAIL}")
    import shutil

    shutil.rmtree(tmp, ignore_errors=True)
    return 1 if FAIL else 0


if __name__ == "__main__":
    sys.exit(main())
