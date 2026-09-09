"""Render captured Flutter Web frames as a narrated 1080p product walkthrough.

Run after capture and demo_video_assets.py --voice. Only reads application builds;
all generated material lives in build/demo-video and dist/demo-video.
"""
from pathlib import Path
import json,re,subprocess,sys,math,hashlib
from PIL import Image,ImageDraw,ImageFont
ROOT=Path(__file__).resolve().parents[1]
BASE=ROOT/'build/demo-video';OUT=ROOT/'dist/demo-video'
FF=ROOT/'build/video-tools/imageio_ffmpeg/binaries/ffmpeg-win-x86_64-v7.1.exe'
SCENES=json.loads((ROOT/'tool/demo_video_scenes.json').read_text(encoding='utf-8-sig'))
OUT.mkdir(parents=True,exist_ok=True)
FONT='C:/Windows/Fonts/msyh.ttc';BOLD='C:/Windows/Fonts/msyhbd.ttc'
INK='#302b46';MUTED='#776f88';ACCENT='#7b63bb'

def run(args,log):
    with open(log,'w',encoding='utf8') as f:
        p=subprocess.run([str(FF),'-hide_banner','-loglevel','warning','-y',*map(str,args)],stdout=f,stderr=f)
    if p.returncode:raise RuntimeError(Path(log).read_text(encoding='utf8')[-4000:])

def probe_duration(p):
    r=subprocess.run([str(FF),'-hide_banner','-i',str(p)],capture_output=True,text=True,encoding='utf8')
    m=re.search(r'Duration: (\d+):(\d+):(\d+\.\d+)',r.stderr)
    if not m:raise ValueError(r.stderr)
    return int(m[1])*3600+int(m[2])*60+float(m[3])

def font(n,bold=False):return ImageFont.truetype(BOLD if bold else FONT,n)
def wrap(text,n):
    lines=[];cur=''
    for c in text:
        if c=='\n':lines.append(cur);cur='';continue
        if len(cur)>=n:lines.append(cur);cur=''
        cur+=c
    if cur:lines.append(cur)
    return lines

def layout(scene,index,caption=''):
    im=Image.new('RGB',(1920,1080),'#f5f3fa');d=ImageDraw.Draw(im)
    # Quiet editorial frame, leaving the actual Flutter app unobscured.
    for y in range(1080):
        f=y/1080;d.line((0,y,1920,y),fill=(int(248-9*f),int(247-10*f),int(251-4*f)))
    d.rounded_rectangle((36,25,94,83),18,fill=ACCENT)
    d.text((46,25),'∞',font=font(42,True),fill='white')
    d.text((110,22),'Morrow',font=font(43,True),fill=INK)
    d.text((326,44),'留一点空间给灵感',font=font(19),fill=MUTED)
    d.text((1270,45),'功能演示  /  0.1.6  ·  Flutter Web',font=font(21),fill=MUTED)
    d.line((36,103,1884,103),fill='#dcd5e8',width=1)
    d.rounded_rectangle((31,125,1482,987),23,fill='#d9d1e4')
    d.rounded_rectangle((34,128,1479,983),21,fill='white')
    d.text((1534,157),scene['tag'],font=font(18,True),fill=ACCENT)
    d.multiline_text((1530,215),scene['title'],font=font(49,True),fill=INK,spacing=18)
    d.rounded_rectangle((1534,399,1598,404),2,fill=ACCENT)
    y=459
    for i,point in enumerate(scene['points']):
        d.ellipse((1534,y+11,1542,y+19),fill=ACCENT)
        lines=wrap(point,13)
        d.multiline_text((1560,y), '\n'.join(lines),font=font(22),fill=INK,spacing=9)
        y+=len(lines)*34+27
    d.text((1534,843),'真实界面 · 演示数据',font=font(18),fill=MUTED)
    d.text((1534,875),'中文合成解说 / 原创配乐',font=font(17),fill=MUTED)
    d.text((1534,934),f'{index+1:02d} / {len(SCENES):02d}',font=font(20,True),fill=ACCENT)
    d.rounded_rectangle((1534,970,1884,975),2,fill='#ddd5e8')
    d.rounded_rectangle((1534,970,1534+350*(index+1)/len(SCENES),975),2,fill=ACCENT)
    if caption:
        lines=wrap(caption,49)
        if len(lines)>2:raise ValueError('Caption too long '+caption)
        y=1011 if len(lines)==1 else 990
        for line in lines:
            w=d.textlength(line,font=font(27))
            d.text(((1920-w)/2,y),line,font=font(27),fill=INK)
            y+=36
    return im

def caption_parts(text):
    # Phrase-sized subtitles keep speech readable without covering controls.
    pieces=re.findall(r'[^。！？；，]+[。！？；，]?',text)
    result=[];part=''
    for x in pieces:
        if len(part+x)>40 and part:result.append(part);part=''
        part+=x
        if len(part)>=23 or x.endswith(('。','！','？')):result.append(part);part=''
    if part:result.append(part)
    return result

def ffconcat(entries,target):
    lines=['ffconcat version 1.0']
    for p,t in entries:
        # All generated paths are controlled and contain no single quote.
        lines+=['file '+"'"+Path(p).resolve().as_posix()+"'",f'duration {t:.8f}']
    lines+=['file '+"'"+Path(entries[-1][0]).resolve().as_posix()+"'"]
    target.write_text('\n'.join(lines)+'\n',encoding='utf8')

def prepare_take(scene):
    takes=sorted((BASE/'takes').glob(scene['id']+'*'))
    if not takes:raise ValueError('Missing capture '+scene['id'])
    frames=[]
    for take in takes:
        if not (take/'frames.json').exists():continue
        data=json.loads((take/'frames.json').read_text())
        for k,item in enumerate(data):
            path=take/item['file']
            dt=min(1.1,max(.008,data[k+1]['time']-item['time'])) if k+1<len(data) else .65
            # Portrait and tablet take frames retain their actual aspect ratio.
            if scene['id']=='11_responsive':
                normal=take/('normalized-'+item['file'])
                if not normal.exists():
                    img=Image.open(path).convert('RGB')
                    if img.size!=(1440,850):
                        ratio=min(1440/img.width,850/img.height)
                        img=img.resize((round(img.width*ratio),round(img.height*ratio)),Image.Resampling.LANCZOS)
                        canvas=Image.new('RGB',(1440,850),'#e9e5f2');canvas.paste(img,((1440-img.width)//2,0));img=canvas
                    img.save(normal,quality=94)
                path=normal
            frames.append([path,dt])
    total=sum(d for _,d in frames)
    # Give each chapter its editorial duration while preserving action order.
    scale=scene['duration']/total
    return [(p,d*scale) for p,d in frames]

def main():
    (BASE/'render').mkdir(exist_ok=True)
    srt=[];global_time=0;manifest=[]
    for index,s in enumerate(SCENES):
        dur=s['duration'];name=s['id'];work=BASE/'render'/name;work.mkdir(exist_ok=True)
        voice=BASE/'voice'/(name+'.mp3');vdur=probe_duration(voice)
        if vdur+.9>dur:
            # Slight speech normalization only if a chapter exceeds its budget.
            ratio=vdur/(dur-1.0)
        else:ratio=1
        effective=vdur/ratio;lead=.45
        parts=caption_parts(s['voice']);total_weight=sum(len(x) for x in parts)
        timeline=[];ts=lead
        empty=work/'layout.png';layout(s,index).save(empty)
        timeline.append((empty,lead))
        for j,cap in enumerate(parts):
            length=effective*len(cap)/total_weight
            p=work/f'caption-{j:02}.png';layout(s,index,cap).save(p)
            timeline.append((p,length));srt.append((global_time+ts,global_time+ts+length,cap));ts+=length
        timeline.append((empty,max(.05,dur-ts)))
        ffconcat(timeline,work/'overlay.ffconcat');ffconcat(prepare_take(s),work/'app.ffconcat')
        segment=work/'segment.mp4'
        if '--prepare-only' not in sys.argv and not (segment.exists() and '--resume' in sys.argv):
            vf=f'[0:v]fps=25,scale=1440:850:flags=lanczos,setsar=1[app];[1:v]fps=25[bg];[bg][app]overlay=36:130:shortest=1,fade=t=in:st=0:d=0.25,fade=t=out:st={dur-.25}:d=0.25,format=yuv420p[v];[2:a]atempo={ratio:.6f},adelay=450,apad,atrim=duration={dur},aformat=sample_rates=48000:channel_layouts=stereo[a]'
            run(['-f','concat','-safe','0','-i',work/'app.ffconcat','-f','concat','-safe','0','-i',work/'overlay.ffconcat','-i',voice,'-filter_complex',vf,'-map','[v]','-map','[a]','-t',dur,'-r','25','-c:v','libx264','-preset','fast','-crf','19','-threads','6','-c:a','aac','-b:a','160k','-movflags','+faststart',segment],work/'render.log')
            print('Rendered',name,dur,'seconds',flush=True)
        manifest.append({'id':name,'start':global_time,'duration':dur,'voice_duration':vdur,'source':'Actual Flutter Web UI capture'})
        global_time+=dur
    def stamp(t):
        ms=round(t*1000);return f'{ms//3600000:02}:{ms//60000%60:02}:{ms//1000%60:02},{ms%1000:03}'
    (OUT/'morrow-功能演示-中文字幕.srt').write_text('\n\n'.join(f'{i+1}\n{stamp(a)} --> {stamp(b)}\n{c}' for i,(a,b,c) in enumerate(srt))+'\n',encoding='utf8')
    (OUT/'chapters.json').write_text(json.dumps(manifest,ensure_ascii=False,indent=2),encoding='utf8')
    (BASE/'render/segments.ffconcat').write_text('ffconcat version 1.0\n'+'\n'.join("file '"+(BASE/'render'/s['id']/'segment.mp4').as_posix()+"'" for s in SCENES)+'\n',encoding='utf8')
    if '--prepare-only' in sys.argv:return
    run(['-f','concat','-safe','0','-i',BASE/'render/segments.ffconcat','-c','copy',BASE/'render/joined.mp4'],BASE/'render/join.log')
    final=OUT/'morrow-功能演示-3分钟-1080p.mp4'
    run(['-i',BASE/'render/joined.mp4','-i',BASE/'assets/留一点空间.wav','-filter_complex','[1:a]volume=0.19,afade=t=in:d=2,afade=t=out:st=176:d=4[bgm];[0:a][bgm]amix=inputs=2:duration=first:normalize=0,alimiter=limit=0.94[a]','-map','0:v:0','-map','[a]','-c:v','copy','-c:a','aac','-b:a','192k','-t',global_time,'-movflags','+faststart','-metadata','title=morrow | 留一点空间给灵感','-metadata','comment=Actual Flutter Web UI. Chinese synthetic narration. Original procedural instrumental music.',final],BASE/'render/mix.log')
    run(['-ss','5','-i',final,'-frames:v','1',OUT/'morrow-功能演示-封面.png'],BASE/'render/poster.log')
    print('FINAL',final,final.stat().st_size,'bytes',flush=True)

if __name__=='__main__':main()
