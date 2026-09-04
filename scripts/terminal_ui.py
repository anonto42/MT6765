#!/usr/bin/env python3
"""Touch-controlled framebuffer terminal for the WalPad."""
import argparse, fcntl, gzip, mmap, os, pty, select, struct, subprocess, time

WIDTH, HEIGHT, STRIDE = 800, 1280, 3200
PAGE = STRIDE * HEIGHT
BG=(10,14,24,255); FG=(230,236,246,255); DIM=(130,145,170,255); GREEN=(74,222,128,255); KEY=(35,48,72,255); ACCENT=(70,110,170,255)
EV=struct.Struct("llHHI"); ROT=0

def read(path, default=""):
    try:
        with open(path, encoding="utf-8") as f: return f.read().strip() or default
    except OSError: return default

def font_load(path):
    with (gzip.open(path,"rb") if path.endswith(".gz") else open(path,"rb")) as f: d=f.read()
    magic,ver,hs,flags,count,gs,h,w=struct.unpack_from("<8I",d)
    return d[hs:hs+count*gs],count,gs,w,h

def mapxy(x,y):
    if ROT==0:return x,y
    if ROT==90:return WIDTH-1-y,x
    if ROT==180:return WIDTH-1-x,HEIGHT-1-y
    return y,HEIGHT-1-x

def px(f,x,y,c):
    w,h=(WIDTH,HEIGHT) if ROT in (0,180) else (HEIGHT,WIDTH)
    if not(0<=x<w and 0<=y<h):return
    x,y=mapxy(x,y); o=y*STRIDE+x*4; f[o:o+4]=bytes((c[2],c[1],c[0],c[3]))

def box(f,x,y,w,h,c):
    for yy in range(y,y+h):
        for xx in range(x,x+w):px(f,xx,yy,c)

def txt(f,ft,s,x,y,c=FG):
    glyphs,count,gs,gw,gh=ft; bpr=(gw+7)//8
    for ch in s:
        n=ord(ch); n=n if n<count else ord('?'); g=glyphs[n*gs:(n+1)*gs]
        for yy in range(gh):
            for xx in range(gw):
                if g[yy*bpr+xx//8]&(0x80>>(xx%8)):px(f,x+xx,y+yy,c)
        x+=(gw+2)

_display_initialized = False
def commit():
    """Present the framebuffer without repeatedly resetting the touch panel.

    The MTK touch driver receives LCD blank notifications.  Toggling blank on
    every render (as the old implementation did) continuously reset the touch
    controller, so it never produced input events.  A single blank/unblank is
    enough to wake the panel; subsequent frames only pan the active page.
    """
    global _display_initialized
    try:
        with open('/sys/class/graphics/fb0/pan','w') as f:f.write('0,1280\n')
        with open('/sys/class/graphics/fb0/pan','w') as f:f.write('0,0\n')
        if not _display_initialized:
            with open('/sys/class/graphics/fb0/blank','w') as f:f.write('1\n')
            time.sleep(0.04)
            with open('/sys/class/graphics/fb0/blank','w') as f:f.write('0\n')
            _display_initialized = True
    except OSError:pass

KEYS=["1234567890", "qwertyuiop", "asdfghjkl", "zxcvbnm", " _-./:;@"]
def main():
    global ROT
    ap=argparse.ArgumentParser(); ap.add_argument('--font',required=True); ap.add_argument('--shell',default='/bin/sh'); a=ap.parse_args()
    ft=font_load(a.font); pid,fd=pty.fork()
    if pid==0: os.execl(a.shell,a.shell,'-l')
    touch=open('/dev/input/event4','rb',buffering=0); sensor=None
    try:sensor=open('/dev/gsensor','rb',buffering=0)
    except OSError:pass
    line=""; output=[]; tx=ty=None; down=False; touch_dirty=False
    with open('/dev/fb0','r+b',buffering=0) as fb, mmap.mmap(fb.fileno(),PAGE*3,mmap.MAP_SHARED,mmap.PROT_READ|mmap.PROT_WRITE) as frame:
      while True:
        if sensor:
          try:
            b=bytearray(128); fcntl.ioctl(sensor,0x80048503,b,True); q=[int(v,16) for v in bytes(b).split(b'\0',1)[0].decode().split()[:2]]
            if len(q)==2: ROT=90 if q[0]>q[1]*1.25 else 270 if q[1]*-1>q[0]*1.25 else 0
          except Exception:pass
        r,_,_=select.select([fd,touch],[],[],.2)
        if fd in r:
          try: data=os.read(fd,4096); output.extend(data.decode(errors='replace').splitlines()); output=output[-34:]
          except OSError:break
        if touch in r:
          data=touch.read(EV.size)
          if len(data)==EV.size:
            _,_,typ,code,val=EV.unpack(data)
            # Accept both single-touch (ABS_X/Y) and protocol-B multitouch
            # coordinates used by mtk-tpd (ABS_MT_POSITION_X/Y).
            if typ==3 and code in (0,53): tx=val; touch_dirty=True
            if typ==3 and code in (1,54): ty=val; touch_dirty=True
            if typ==3 and code==57:  # ABS_MT_TRACKING_ID
              if val < 0: down=False
              else: down=True
            if typ==1 and code in (330, BTN_TOUCH:=330):
              if val==1: down=True
              elif down and tx is not None and ty is not None:
                down=False; touch_dirty=False; sw,sh=(WIDTH,HEIGHT) if ROT in (0,180) else (HEIGHT,WIDTH); x=tx*sw//800; y=ty*sh//1280
                # Enter is a dedicated strip above the keyboard.
                # evdev reports the panel's native 800x1280 coordinates.
                px,py=tx*WIDTH//800,ty*HEIGHT//1280
                if ROT==0: x,y=px,py
                elif ROT==90: x,y=py,WIDTH-1-px
                elif ROT==180: x,y=WIDTH-1-px,HEIGHT-1-py
                else: x,y=HEIGHT-1-py,px
                if y>sh-330 and y<=sh-290 and x>sw-190:
                  os.write(fd,b'\n'); line=''
                elif y>sh-290:
                  row=min(4,max(0,(y-(sh-290))*5//290)); col=min(len(KEYS[row])-1,max(0,x*len(KEYS[row])//sw)); ch=KEYS[row][col]
                  if ch=='_':ch=' '
                  line+=ch; os.write(fd,ch.encode())
            # Some firmware builds omit BTN_TOUCH release and terminate a
            # contact with SYN_REPORT after ABS_MT_TRACKING_ID=-1. The
            # tracking-id path above still keeps the normal release behavior.
        frame[:PAGE]=bytes((BG[2],BG[1],BG[0],255))*(WIDTH*HEIGHT)
        sw,sh=(WIDTH,HEIGHT) if ROT in (0,180) else (HEIGHT,WIDTH); rows=max(1,(sh-330)//24)
        txt(frame,ft,'TOUCH TERMINAL',18,18,GREEN)
        txt(frame,ft,'root@walpad  interactive shell',18,52,DIM)
        for i,s in enumerate(output[-rows:]):txt(frame,ft,s[:sw//14],18,95+i*24,DIM)
        box(frame,12,sh-325,sw-24,2,ACCENT); txt(frame,ft,'$ '+line,20,sh-305,GREEN)
        box(frame, sw-190, sh-330, 170, 30, ACCENT); txt(frame,ft,'ENTER',sw-165,sh-324,FG)
        for ri,row in enumerate(KEYS):
          y=sh-270+ri*52; kw=(sw-36)//len(row)
          for ci,ch in enumerate(row): box(frame,18+ci*kw,y,kw-5,42,KEY); txt(frame,ft,ch,28+ci*kw,y+9,FG)
        frame[PAGE:PAGE*2]=frame[:PAGE]; frame[PAGE*2:PAGE*3]=frame[:PAGE]; commit()
    os.close(fd)

if __name__=='__main__':main()
