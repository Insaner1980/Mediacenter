"""Disposable native visual smoke. Never touches the user's desktop or configuration."""
import ctypes, ctypes.util, json, os, pathlib, re, secrets, signal, sqlite3, subprocess, tempfile, time
root=pathlib.Path(__file__).resolve().parents[4]
out=root/'docs/engineering/evidence/simplified-ui'
private=pathlib.Path(tempfile.mkdtemp(prefix='mediacenter-simplified-native-'))
for name in ('data','config','cache','state','runtime'):(private/name).mkdir(mode=0o700)
display=next(f':{n}' for n in range(187,250) if not pathlib.Path(f'/tmp/.X11-unix/X{n}').exists() and not pathlib.Path(f'/tmp/.X{n}-lock').exists())
auth=private/'Xauthority';auth.touch(mode=0o600)
subprocess.run(['xauth','-f',str(auth),'add',display,'.',secrets.token_hex(16)],check=True,capture_output=True)
env=dict(os.environ,DISPLAY=display,XAUTHORITY=str(auth),WAYLAND_DISPLAY='',GDK_BACKEND='x11',GIO_USE_VFS='local',GTK_USE_PORTAL='0',WEBKIT_DISABLE_COMPOSITING_MODE='1',WEBKIT_DISABLE_DMABUF_RENDERER='1',XDG_SESSION_TYPE='x11',XDG_DATA_HOME=str(private/'data'),XDG_CONFIG_HOME=str(private/'config'),XDG_CACHE_HOME=str(private/'cache'),XDG_STATE_HOME=str(private/'state'),XDG_RUNTIME_DIR=str(private/'runtime'),PULSE_SERVER='unix:'+str(private/'no-audio-server'))
log=(private/'display.log').open('w');xvfb=subprocess.Popen([str(root/'.tools/xvfb/root/usr/bin/Xvfb'),display,'-screen','0','1440x900x24','-nolisten','tcp','-auth',str(auth)],stdout=log,stderr=log,start_new_session=True)
def stop(p):
 if p.poll() is None:
  os.killpg(p.pid,signal.SIGTERM)
  try:p.wait(timeout=5)
  except subprocess.TimeoutExpired:os.killpg(p.pid,signal.SIGKILL);p.wait()
def capture(name):
 subprocess.run(['xrefresh'],env=env,capture_output=True,check=True)
 time.sleep(1)
 subprocess.run(['ffmpeg','-hide_banner','-loglevel','error','-f','x11grab','-video_size','1440x900','-i',display,'-frames:v','1','-y',str(out/name)],env=env,check=True,capture_output=True,timeout=15)
record={'localChanges':bool(subprocess.check_output(['git','diff','--name-only','--','src','vite.config.ts'],cwd=root,text=True).strip()),'scope':'Native release executable, private authenticated Xvfb and session bus, private XDG directories, no real media or device/network/output mutations','source':subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),'binary':'src-tauri/target/release/mediacenter','display':[1440,900],'renderer':'WebKitGTK with software compositing for isolated Xvfb capture','themes':[]}
try:
 for _ in range(50):
  if subprocess.run(['xwininfo','-root'],env=env,capture_output=True).returncode==0:break
  time.sleep(.1)
 else:raise RuntimeError('Private X server not ready')
 db=private/'data/io.emma.mediacenter/library.db'
 for theme in ['dark','light']:
  if db.exists():
   with sqlite3.connect(db) as connection:connection.execute("INSERT INTO settings(key,value_json,updated_at_ms) VALUES('theme',?,?) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json,updated_at_ms=excluded.updated_at_ms",(json.dumps(theme),int(time.time()*1000)))
  appLog=(out/f'native-{theme}.log').open('w')
  app=subprocess.Popen(['dbus-run-session','--',str(root/'src-tauri/target/release/mediacenter')],env=env,stdout=appLog,stderr=subprocess.STDOUT,start_new_session=True)
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
   click(140,310);capture(f'native-{theme}-videos.png')
   click(1140,166);capture(f'native-{theme}-dialog.png')
   xt.XTestFakeKeyEvent(d,9,1,0);xt.XTestFakeKeyEvent(d,9,0,0);x11.XFlush(d);time.sleep(1)
   x11.XResizeWindow(d,int(wid,16),800,600);x11.XFlush(d);time.sleep(2);capture(f'native-{theme}-800.png');x11.XCloseDisplay(d)
   if old_auth is None:os.environ.pop('XAUTHORITY',None)
   else:os.environ['XAUTHORITY']=old_auth
   record['themes'].append({'theme':theme,'windowPresent':True,'processAlive':app.poll() is None,'captures':[f'native-{theme}-1280.png',f'native-{theme}-800.png']})
  finally:stop(app);appLog.close()
 record['result']='process, window, asset capture and resize completed; see manual screenshot review'
finally:
 stop(xvfb);log.close();(out/'native-smoke.json').write_text(json.dumps(record,indent=2)+'\n')
print(json.dumps(record,indent=2))
