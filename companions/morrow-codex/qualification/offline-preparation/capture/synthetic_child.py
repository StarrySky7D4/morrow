"""Fixed synthetic capture fixture, no arguments or external input."""
import os
import sys
import time
if len(sys.argv)!=1: raise SystemExit(19)
for _ in range(32):
    os.write(1,b'O'*4096)
    os.write(2,b'E'*4096)
time.sleep(0.2)
raise SystemExit(0)
