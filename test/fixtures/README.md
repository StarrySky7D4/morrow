# Media QA fixtures

`texture.png` and `motion.gif` are generated test artwork: a color gradient,
frame counter and moving circle. The PNG is used by the canvas sizing regression.

`motion.mp4` is an optional local manual-testing download, excluded from Git and
from the app bundle. It comes from the video example linked in media_kit's
official README:
https://user-images.githubusercontent.com/28951144/229373695-22f88f13-d18f-4288-9bf1-c3e078d83722.mp4

Manual checks cover image/GIF/video import, playback/pause, restart recovery,
dialog transparency, and changing background modes while video is playing.
