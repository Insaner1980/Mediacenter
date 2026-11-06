"""Native playback regression using generated media, private Xvfb and hardware-free audio.

Run after a release build: python3 tests/native/reference_feedback.py
MEDIACENTER_NATIVE_BINARY can select an extracted .deb executable.
MEDIACENTER_NATIVE_EVIDENCE selects the output directory (default: new /tmp directory).
Requires Xvfb, ffmpeg, mpv, PipeWire, WirePlumber 0.4, xauth, xwininfo, Pillow.
"""
import ctypes, ctypes.util, json, os, pathlib, re, secrets, signal, sqlite3, subprocess, tempfile, time
root=pathlib.Path(__file__).resolve().parents[2]
binary=pathlib.Path(os.environ.get('MEDIACENTER_NATIVE_BINARY',str(root/'src-tauri/target/release/mediacenter')))
out=pathlib.Path(os.environ.get('MEDIACENTER_NATIVE_EVIDENCE') or tempfile.mkdtemp(prefix='mediacenter-playback-evidence-'))
out.mkdir(parents=True,exist_ok=True)
private=pathlib.Path(tempfile.mkdtemp(prefix='mediacenter-simplified-native-'))
for name in ('data','config','cache','state','runtime','Pictures','Videos','Music'):(private/name).mkdir(mode=0o700)
# Synthetic files and XDG paths exist only in this disposable configuration.
(private/'config/user-dirs.dirs').write_text(''.join(f'XDG_{kind}_DIR="{private/folder}"\n' for kind,folder in [('PICTURES','Pictures'),('VIDEOS','Videos'),('MUSIC','Music')]))
from PIL import Image,ImageDraw
im=Image.new('RGB',(640,360),'#20465b');draw=ImageDraw.Draw(im);draw.rectangle((0,220,640,360),fill='#708d67');draw.ellipse((420,40,520,140),fill='#e9c976');im.save(private/'Pictures/Native sample.png')
import wave
with wave.open(str(private/'Music/Native sample.wav'),'wb') as wav:
 wav.setnchannels(1);wav.setsampwidth(2);wav.setframerate(8000);wav.writeframes(b'\0\0'*8000*60)
subprocess.run(['ffmpeg','-hide_banner','-loglevel','error','-f','lavfi','-i','testsrc2=size=320x180:rate=10','-t','60','-c:v','libx264','-y',str(private/'Videos/Native sample.mp4')],check=True,capture_output=True)

import shutil
shutil.copyfile(private/'Videos/Native sample.mp4',private/'Videos/Other sample.mp4')
subprocess.run(['ffmpeg','-hide_banner','-loglevel','error','-f','lavfi','-i','sine=frequency=440:sample_rate=48000','-t','60','-ac','2','-y',str(private/'Music/Native sample.wav')],check=True,capture_output=True)
shutil.copyfile(private/'Music/Native sample.wav',private/'Music/Other sample.wav')

# Add the MP3-with-artwork case missing from the earlier WAV-only smoke.
(private/'Music/Native sample.wav').rename(private/'Music/01 Plain.wav')
subprocess.run(['ffmpeg','-hide_banner','-loglevel','error','-i',str(private/'Music/01 Plain.wav'),'-i',str(private/'Pictures/Native sample.png'),'-map','0:a','-map','1:v','-c:a','libmp3lame','-q:a','4','-c:v','png','-id3v2_version','3','-disposition:v','attached_pic','-metadata','title=Test cover track','-y',str(private/'Music/02 Cover.mp3')],check=True,capture_output=True)
display=next(f':{n}' for n in range(187,250) if not pathlib.Path(f'/tmp/.X11-unix/X{n}').exists() and not pathlib.Path(f'/tmp/.X{n}-lock').exists())
auth=private/'Xauthority';auth.touch(mode=0o600)
subprocess.run(['xauth','-f',str(auth),'add',display,'.',secrets.token_hex(16)],check=True,capture_output=True)
env=dict(os.environ,DISPLAY=display,XAUTHORITY=str(auth),WAYLAND_DISPLAY='',GDK_BACKEND='x11',GIO_USE_VFS='local',GTK_USE_PORTAL='0',WEBKIT_DISABLE_COMPOSITING_MODE='1',WEBKIT_DISABLE_DMABUF_RENDERER='1',XDG_SESSION_TYPE='x11',XDG_DATA_HOME=str(private/'data'),XDG_CONFIG_HOME=str(private/'config'),XDG_CACHE_HOME=str(private/'cache'),XDG_STATE_HOME=str(private/'state'),XDG_RUNTIME_DIR=str(private/'runtime'),PULSE_SERVER='unix:'+str(private/'runtime/pulse/native'),PIPEWIRE_RUNTIME_DIR=str(private/'runtime'),PIPEWIRE_REMOTE='pipewire-0')
log=(private/'display.log').open('w');xvfb=subprocess.Popen([str(root/'.tools/xvfb/root/usr/bin/Xvfb'),display,'-screen','0','1440x900x24','-nolisten','tcp','-auth',str(auth)],stdout=log,stderr=log,start_new_session=True)
def stop(p):
 if p.poll() is None:
  os.killpg(p.pid,signal.SIGTERM)
  try:p.wait(timeout=5)
  except subprocess.TimeoutExpired:os.killpg(p.pid,signal.SIGKILL);p.wait()
def capture(name):
 # xrefresh can invalidate an mpv GL surface on headless Xvfb. Do not
 # mutate the display while measuring playback; capture its actual pixels.
 tree=subprocess.check_output(['xwininfo','-root','-tree'],env=env,text=True)
 if '"mpv"' not in tree:subprocess.run(['xrefresh'],env=env,capture_output=True,check=True)
 time.sleep(1)
 subprocess.run(['ffmpeg','-hide_banner','-loglevel','error','-f','x11grab','-video_size','1440x900','-i',display,'-frames:v','1','-y',str(out/name)],env=env,check=True,capture_output=True,timeout=15)
audio_processes=[]
record={'localChanges':bool(subprocess.check_output(['git','status','--porcelain','--','src','src-tauri','package.json','package-lock.json','vite.config.ts'],cwd=root,text=True).strip()),'scope':'Native release executable, synthetic media in private XDG system folders, private Xvfb/session bus; no user media or host device/network/output mutations; private null audio output only','source':subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),'binary':str(binary),'binarySha256':__import__('hashlib').sha256(binary.read_bytes()).hexdigest(),'version':json.loads((root/'package.json').read_text())['version'],'display':[1440,900],'renderer':'WebKitGTK with software compositing for isolated Xvfb capture','themes':[]}
try:
 for _ in range(50):
  if subprocess.run(['xwininfo','-root'],env=env,capture_output=True).returncode==0:break
  time.sleep(.1)
 else:raise RuntimeError('Private X server not ready')
 # Private audio server with a null sink; it cannot address physical devices.
 audio_env=dict(env,DBUS_SESSION_BUS_ADDRESS='unix:path='+str(private/'no-bus'))
 pw=subprocess.Popen(['pipewire'],env=audio_env,stdout=(out/'pipewire.log').open('w'),stderr=subprocess.STDOUT,start_new_session=True);audio_processes.append(pw)
 pulse=subprocess.Popen(['pipewire-pulse'],env=audio_env,stdout=(out/'pulse.log').open('w'),stderr=subprocess.STDOUT,start_new_session=True);audio_processes.append(pulse)
 for _ in range(50):
  ready=subprocess.run(['pactl','info'],env=audio_env,capture_output=True)
  if ready.returncode==0:break
  time.sleep(.1)
 subprocess.run(['pactl','load-module','module-null-sink','sink_name=mediatest'],env=audio_env,check=True,capture_output=True)
 wpconfig=private/'config/wireplumber'
 shutil.copytree('/usr/share/wireplumber',wpconfig)
 conf=(wpconfig/'wireplumber.conf').read_text().replace('{ name = bluetooth.lua, type = config/lua }','')
 (wpconfig/'wireplumber.conf').write_text(conf)
 (wpconfig/'main.lua.d/80-test-only.lua').write_text('alsa_monitor.enabled = false\nv4l2_monitor.enabled = false\nlibcamera_monitor.enabled = false\n')
 wp_env=dict(audio_env,WIREPLUMBER_CONFIG_DIR=str(wpconfig))
 wp=subprocess.Popen(['dbus-run-session','--','wireplumber'],env=wp_env,stdout=(out/'wireplumber.log').open('w'),stderr=subprocess.STDOUT,start_new_session=True);audio_processes.append(wp)
 time.sleep(2)
 assert wp.poll() is None,'Private audio policy stopped'
 subprocess.run(['pactl','set-default-sink','mediatest'],env=audio_env,check=True,capture_output=True)
 assert subprocess.check_output(['pactl','get-default-sink'],env=audio_env,text=True).strip()=='mediatest'
 db=private/'data/io.emma.mediacenter/library.db'
 for theme in ['dark']:
  if db.exists():
   with sqlite3.connect(db) as connection:connection.execute("INSERT INTO settings(key,value_json,updated_at_ms) VALUES('theme',?,?) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json,updated_at_ms=excluded.updated_at_ms",(json.dumps(theme),int(time.time()*1000)))
  appLog=(out/f'native-{theme}.log').open('w')
  app=subprocess.Popen(['dbus-run-session','--',str(binary)],env=env,stdout=appLog,stderr=subprocess.STDOUT,start_new_session=True)
  try:
   for _ in range(120):
    if app.poll() is not None:raise RuntimeError('Native process exited')
    tree=subprocess.check_output(['xwininfo','-root','-tree'],env=env,text=True)
    (out/'native-window-tree.txt').write_text(tree)
    found=re.search(r'(0x[0-9a-f]+) "Mediacenter[^\"]*".*? (1280x800)',tree)
    if found and db.exists():break
    time.sleep(.5)
   else:raise RuntimeError('Native window absent')
   time.sleep(6)
   wid=found.group(1)
   # XResizeWindow acts only on the window on this authenticated private display.
   x11=ctypes.CDLL(ctypes.util.find_library('X11'));x11.XOpenDisplay.argtypes=[ctypes.c_char_p];x11.XOpenDisplay.restype=ctypes.c_void_p
   old_auth=os.environ.get('XAUTHORITY');os.environ['XAUTHORITY']=str(auth)
   d=x11.XOpenDisplay(display.encode())
   if not d:raise RuntimeError('Cannot open private display')
   x11.XResizeWindow.argtypes=[ctypes.c_void_p,ctypes.c_ulong,ctypes.c_uint,ctypes.c_uint];x11.XFlush.argtypes=[ctypes.c_void_p];x11.XCloseDisplay.argtypes=[ctypes.c_void_p]
   x11.XResizeWindow(d,int(wid,16),1280,800);x11.XFlush(d);time.sleep(2);capture(f'native-{theme}-1280.png')
   # Exercise a real native dialog using input on the private X server only.
   xt=ctypes.CDLL(ctypes.util.find_library('Xtst'));xt.XTestFakeMotionEvent.argtypes=[ctypes.c_void_p,ctypes.c_int,ctypes.c_int,ctypes.c_int,ctypes.c_ulong];xt.XTestFakeButtonEvent.argtypes=[ctypes.c_void_p,ctypes.c_uint,ctypes.c_int,ctypes.c_ulong];xt.XTestFakeKeyEvent.argtypes=[ctypes.c_void_p,ctypes.c_uint,ctypes.c_int,ctypes.c_ulong]
   def click(x,y):
    xt.XTestFakeMotionEvent(d,-1,x,y,0);xt.XTestFakeButtonEvent(d,1,1,0);xt.XTestFakeButtonEvent(d,1,0,0);x11.XFlush(d);time.sleep(1)
   def verify_index(kind):
    for _ in range(60):
     with sqlite3.connect(db) as connection:count=connection.execute('SELECT COUNT(*) FROM media_items WHERE kind=? AND availability=?',(kind,'available')).fetchone()[0]
     if count>={'video':2,'audio':2,'photo':1}[kind]:break
     time.sleep(.5)
    else:
     capture('native-index-error.png');raise RuntimeError('Automatic indexing did not find synthetic '+kind)
    time.sleep(2)
   def players():
    result=[]
    for path in pathlib.Path('/proc').glob('[0-9]*/environ'):
     try:
      if (path.parent/'comm').read_text().strip()!='mpv':continue
      values=path.read_bytes().split(b'\0')
      if ('DISPLAY='+display).encode() in values:result.append(int(path.parent.name))
     except (OSError,PermissionError):pass
    return result
   def wait_player():
    for _ in range(30):
     pids=players()
     if len(pids)==1:return pids[0]
     time.sleep(.3)
    capture('failed.png');raise AssertionError('No new mpv process: playback did not start')
   x11.XSetInputFocus.argtypes=[ctypes.c_void_p,ctypes.c_ulong,ctypes.c_int,ctypes.c_ulong]
   x11.XKeysymToKeycode.argtypes=[ctypes.c_void_p,ctypes.c_ulong];x11.XKeysymToKeycode.restype=ctypes.c_uint
   x11.XRaiseWindow.argtypes=[ctypes.c_void_p,ctypes.c_ulong]
   def app_click(x,y):
    x11.XRaiseWindow(d,int(wid,16));x11.XSetInputFocus(d,int(wid,16),1,0);x11.XFlush(d);click(x,y)
   def close_movie(window_close=False):
    tree=subprocess.check_output(['xwininfo','-root','-tree'],env=env,text=True)
    movie=re.search(r'(0x[0-9a-f]+).*\("[^"]+" "mpv"\)',tree)
    assert movie,tree
    movie_id=int(movie.group(1),16)
    if window_close:
     class Data(ctypes.Union):_fields_=[('l',ctypes.c_long*5)]
     class Message(ctypes.Structure):_fields_=[('type',ctypes.c_int),('serial',ctypes.c_ulong),('send_event',ctypes.c_int),('display',ctypes.c_void_p),('window',ctypes.c_ulong),('message_type',ctypes.c_ulong),('format',ctypes.c_int),('data',Data)]
     class Event(ctypes.Union):_fields_=[('client',Message),('pad',ctypes.c_long*24)]
     x11.XInternAtom.argtypes=[ctypes.c_void_p,ctypes.c_char_p,ctypes.c_int];x11.XInternAtom.restype=ctypes.c_ulong
     x11.XSendEvent.argtypes=[ctypes.c_void_p,ctypes.c_ulong,ctypes.c_int,ctypes.c_long,ctypes.POINTER(Event)]
     event=Event();event.client.type=33;event.client.display=d;event.client.window=movie_id;event.client.message_type=x11.XInternAtom(d,b'WM_PROTOCOLS',0);event.client.format=32;event.client.data.l[0]=x11.XInternAtom(d,b'WM_DELETE_WINDOW',0)
     assert x11.XSendEvent(d,movie_id,0,0,ctypes.byref(event))
    else:
     x11.XSetInputFocus(d,movie_id,1,0)
     key=x11.XKeysymToKeycode(d,ord('q'));xt.XTestFakeKeyEvent(d,key,1,0);xt.XTestFakeKeyEvent(d,key,0,0)
    x11.XFlush(d);time.sleep(1)
    assert not players(),'mpv did not close'
   def position(name):
    with sqlite3.connect(db) as c:
     row=c.execute('SELECT r.position_ms FROM resume_positions r JOIN media_items m ON m.id=r.media_id WHERE m.display_name=?',(name,)).fetchone()
     return row[0] if row else -1
   def progress(name,minimum=2000,maximum=60000):
    for _ in range(35):
     value=position(name)
     if minimum<=value<maximum:return value
     time.sleep(.3)
    capture('failed-progress.png');raise AssertionError(f'Playback position {name}: {value}, expected {minimum}..{maximum}')
   def changed_player(previous):
    for _ in range(35):
     p=players()
     if len(p)==1 and p[0]!=previous:return p[0]
     time.sleep(.3)
    capture('failed-switch.png');raise AssertionError('Player did not switch')
   def link_audio():
    for _ in range(50):
     streams=json.loads(subprocess.check_output(['pactl','-f','json','list','sink-inputs'],env=env,text=True))
     # This private server has no hardware or other audio clients. Native PipeWire mpv
     # omits application.process.id from pactl's stream properties.
     owned=[stream for stream in streams if stream.get('properties',{}).get('application.name')=='mpv' and len(players())==1]
     if owned:break
     time.sleep(.2)
    if not owned:
     capture('music-no-stream.png');(out/'no-streams.json').write_text(json.dumps({'streams':streams,'players':players()}))
    assert len(streams)==1 and len(owned)==1,'Expected one mpv stream on the isolated test server'
    sinks=json.loads(subprocess.check_output(['pactl','-f','json','list','sinks'],env=env,text=True))
    assert len(sinks)==1 and sinks[0]['name']=='mediatest',sinks
    assert all(stream['sink']==sinks[0]['index'] for stream in owned)
    record['audioSink']='private PipeWire null sink, one mpv stream on isolated server; no speaker listening test'
   def key(symbol,window=None):
    if window is not None:x11.XSetInputFocus(d,window,1,0)
    code=x11.XKeysymToKeycode(d,symbol);xt.XTestFakeKeyEvent(d,code,1,0);xt.XTestFakeKeyEvent(d,code,0,0);x11.XFlush(d);time.sleep(.4)
   def player_window():
    for _ in range(30):
     tree=subprocess.check_output(['xwininfo','-root','-tree'],env=env,text=True)
     windows=re.findall(r'(0x[0-9a-f]+).*\("[^"]+" "mpv"\)',tree)
     if len(windows)==1:return int(windows[0],16)
     time.sleep(.2)
    capture('missing-music-window.png');raise AssertionError('Music needs exactly one visible player window, including without artwork')
   click(140,310);verify_index('video');app_click(100,330);verify_index('audio')
   app_click(450,447);plain=wait_player();window=player_window();link_audio();record['plainPositionMs']=progress('01 Plain.wav');capture('plain-music.png')
   key(32,window);time.sleep(6);paused=position('01 Plain.wav');time.sleep(3);assert abs(position('01 Plain.wav')-paused)<500
   key(32,window);record['resumedPlainPositionMs']=progress('01 Plain.wav',paused+1000)
   app_click(450,503);cover=changed_player(plain);player_window();link_audio();record['coverPositionMs']=progress('02 Cover.mp3');capture('cover-music.png')
   close_movie(window_close=True);app_click(450,447);plain2=wait_player();window=player_window();link_audio();time.sleep(2);app_click(1166,503);key(0xff52,window);queued=changed_player(plain2);player_window();link_audio();capture('queued-cover.png');close_movie();record['queueAutoAdvance']='PASS: native player seek to EOF advances to queued cover track'
   record['musicWindows']='PASS: plain WAV and MP3 with embedded artwork; pause/resume, replace, close/replay, one mpv window'
   # Same output popup on a library, Settings, and Home; Escape returns to the page.
   for route,name in [(None,'library'),((100,603),'settings'),((100,178),'home')]:
    if route:app_click(*route)
    app_click(*( (980,108) if name=='home' else (1100,70) ));capture('output-'+name+'.png');key(0xff1b)
   # Photos retains only the dialog close, and management is absent from its header.
   click(140,310);app_click(100,405);verify_index('photo');app_click(330,410);capture('photo-viewer.png');key(0xff1b);capture('photos.png')
   app_click(100,603);capture('settings.png')
   record['uiChecks']='Output popup and photo viewer captured on native WebKitGTK; behavior and focus assertions covered by browser/unit tests'
   x11.XResizeWindow(d,int(wid,16),800,600);x11.XFlush(d);time.sleep(2);capture(f'native-{theme}-800.png');x11.XCloseDisplay(d)
   if old_auth is None:os.environ.pop('XAUTHORITY',None)
   else:os.environ['XAUTHORITY']=old_auth
   record['themes'].append({'theme':theme,'windowPresent':True,'processAlive':app.poll() is None,'captures':[f'native-{theme}-1280.png',f'native-{theme}-800.png']})
  finally:stop(app);appLog.close()
 record['result']='PASS: real native playback transitions, saved advancing positions, single owned mpv, generated media; screenshots reviewed separately'
finally:
 for process in reversed(audio_processes):stop(process)
 stop(xvfb);log.close();(out/'native-smoke.json').write_text(json.dumps(record,indent=2)+'\n')
print(json.dumps(record,indent=2))
