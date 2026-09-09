"""Generate original demo audio, captions and Chinese synthetic narration."""
import asyncio,json,sys,wave,math
from pathlib import Path
import numpy as np
ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'build/video-tools'))
BASE=ROOT/'build/demo-video'
ASSETS=BASE/'assets'
SCENES=json.loads((ROOT/'tool/demo_video_scenes.json').read_text(encoding='utf8'))

def assets():
    ASSETS.mkdir(exist_ok=True,parents=True)
    sr=24000; duration=180
    t=np.arange(sr*duration,dtype=np.float64)/sr
    song=np.zeros_like(t)
    chords=[[48,55,60,64,67],[45,52,57,60,64],[41,48,53,57,60],[43,50,55,59,62]]
    for k in range(0,duration,4):
        ch=chords[(k//4)%4]
        for j,m in enumerate(ch):
            start=k+j*.47; rel=t-start; mask=(rel>=0)&(rel<5)
            q=rel[mask]; f=440*2**((m-69)/12)
            song[mask]+=(np.sin(2*np.pi*f*q)+.24*np.sin(2*np.pi*2*f*q)+.07*np.sin(2*np.pi*3*f*q))*np.exp(-q/1.2)*np.minimum(q/.03,1)*.065
    song*=np.minimum(t/3,1)*np.minimum((duration-t)/4,1)
    with wave.open(str(ASSETS/'留一点空间.wav'),'wb') as out:
        out.setnchannels(1);out.setsampwidth(2);out.setframerate(sr)
        out.writeframes((np.clip(song,-1,1)*32767).astype('<i2').tobytes())
    (ASSETS/'留一点空间.lrc').write_text('[00:00.00]留一点空间，给灵感\n[00:04.00]把今天的光，轻轻收藏\n[00:08.00]一个念头，一小步成长\n[00:12.00]慢一点，也是在向前\n[00:16.00]让想法，自由生长\n[00:22.00]从一个小小的念头开始',encoding='utf8')
    (ASSETS/'光影札记.txt').write_text('周末光影小册\n素材：窗边的光、散步时的树影、傍晚的天空。\n下一步：选三张图，做第一张卡片。',encoding='utf8')
    (ASSETS/'光影卡片.svg').write_text('''<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="800" viewBox="0 0 1200 800"><defs><linearGradient id="g" x2="1" y2="1"><stop stop-color="#c7b4eb"/><stop offset=".5" stop-color="#efe3dc"/><stop offset="1" stop-color="#a6c9c4"/></linearGradient></defs><rect width="1200" height="800" fill="url(#g)"/><circle cx="880" cy="230" r="140" fill="#fff5ce"/><path d="M0 550 Q250 300 540 520 T1200 470 V800 H0" fill="#718c8f"/><path d="M0 650 Q360 480 660 650 T1200 560 V800 H0" fill="#4c666d"/><rect x="62" y="62" width="1076" height="676" rx="25" fill="none" stroke="white" stroke-opacity=".55"/><text x="92" y="151" font-family="Segoe UI" font-size="22" letter-spacing="8" fill="#454056">MORROW / LITTLE MOMENTS</text><text x="90" y="275" font-family="Microsoft YaHei" font-size="67" fill="#454056">把周末的光装进口袋</text><text x="94" y="332" font-family="Microsoft YaHei" font-size="26" fill="#62566e">一张图，也可以是一个念头的开始。</text></svg>''',encoding='utf8')
    print('Original demo assets ready',flush=True)

async def narrate():
    import edge_tts
    out=BASE/'voice';out.mkdir(exist_ok=True)
    limit=asyncio.Semaphore(3)
    async def one(s):
        async with limit:
            target=out/(s['id']+'.mp3')
            if target.exists() and target.stat().st_size>1000:return
            await edge_tts.Communicate(s['voice'],'zh-CN-XiaoxiaoNeural',rate='+6%').save(str(target))
            print('Narration',s['id'],flush=True)
    await asyncio.gather(*(one(s) for s in SCENES))

if __name__=='__main__':
    if '--voice' in sys.argv:asyncio.run(narrate())
    else:assets()
