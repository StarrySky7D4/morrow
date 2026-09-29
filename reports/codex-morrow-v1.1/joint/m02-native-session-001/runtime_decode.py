"""Independent capnp CLI decoding without opening a console window."""
import subprocess
from review_capnp_kit import payload,parse_text,CAPNP,SCHEMA
def decode_frame(frame):
    proc=subprocess.run([str(CAPNP),'decode',str(SCHEMA),'NativeSession'],input=payload(frame),capture_output=True,timeout=10,creationflags=subprocess.CREATE_NO_WINDOW)
    if proc.returncode:raise ValueError('independent capnp decoder rejected')
    return parse_text(proc.stdout.decode('utf-8')),proc.stdout,proc.stderr
