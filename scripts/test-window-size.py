"""Exercise only our disposable Xpeed process, never an existing user window."""
import ctypes, subprocess, time, sys
from ctypes import wintypes as w
from pathlib import Path
u=ctypes.WinDLL('user32',use_last_error=True)
u.SetProcessDpiAwarenessContext.argtypes=[ctypes.c_void_p]
u.SetProcessDpiAwarenessContext(ctypes.c_void_p(-4))
u.GetWindowRect.argtypes=[w.HWND,ctypes.POINTER(w.RECT)]
u.GetWindowThreadProcessId.argtypes=[w.HWND,ctypes.POINTER(w.DWORD)]
u.GetDpiForWindow.argtypes=[w.HWND]
u.GetWindowTextW.argtypes=[w.HWND,w.LPWSTR,ctypes.c_int]
u.SetWindowPos.argtypes=[w.HWND,w.HWND,ctypes.c_int,ctypes.c_int,ctypes.c_int,ctypes.c_int,w.UINT]
u.ShowWindow.argtypes=[w.HWND,ctypes.c_int]
u.IsZoomed.argtypes=[w.HWND]
u.GetMonitorInfoW.argtypes=[w.HANDLE,ctypes.c_void_p]
class Info(ctypes.Structure):
    _fields_=[('size',w.DWORD),('monitor',w.RECT),('work',w.RECT),('flags',w.DWORD)]
monitors=[]
MonitorCallback=ctypes.WINFUNCTYPE(w.BOOL,w.HANDLE,w.HDC,ctypes.POINTER(w.RECT),w.LPARAM)
def each_monitor(handle,dc,rect,data):
    info=Info()
    info.size=ctypes.sizeof(info)
    assert u.GetMonitorInfoW(handle,ctypes.byref(info))
    monitors.append((info.work.left,info.work.top,info.work.right,info.work.bottom))
    return True
u.EnumDisplayMonitors(None,None,MonitorCallback(each_monitor),0)
print('Monitor work areas:',monitors)
if len(sys.argv)>1 and sys.argv[1]=='--list': sys.exit(0)
root=Path(__file__).resolve().parent.parent
exe=root/(sys.argv[1] if len(sys.argv)>1 else 'target/debug/xpeed.exe')
process=subprocess.Popen([str(exe),'--window-test'],creationflags=0x08000000,stdout=subprocess.DEVNULL,stderr=subprocess.PIPE)
WindowCallback=ctypes.WINFUNCTYPE(w.BOOL,w.HWND,w.LPARAM)
def find_window():
    found=[]
    def each(hwnd,data):
        pid=w.DWORD()
        u.GetWindowThreadProcessId(hwnd,ctypes.byref(pid))
        title=ctypes.create_unicode_buffer(256)
        u.GetWindowTextW(hwnd,title,256)
        if pid.value==process.pid and u.IsWindowVisible(hwnd) and title.value.startswith('Xpeed'): found.append(hwnd)
        return True
    u.EnumWindows(WindowCallback(each),0)
    return found[0] if found else None
def geometry(hwnd):
    rect=w.RECT()
    assert u.GetWindowRect(hwnd,ctypes.byref(rect))
    return rect.left,rect.top,rect.right,rect.bottom
def fits(rect,work):
    x,y,r,b=rect
    left,top,right,bottom=work
    return x>=left and y>=top and r<=right and b<=bottom
try:
    hwnd=None
    deadline=time.time()+10
    while not hwnd and time.time()<deadline:
        hwnd=find_window()
        time.sleep(.05)
    assert hwnd,'No test window'
    time.sleep(.5)
    print('Initial:',geometry(hwnd),'dpi:',u.GetDpiForWindow(hwnd))
    assert any(fits(geometry(hwnd),m) for m in monitors),'Startup did not fit a monitor'
    for index,work in enumerate(monitors):
        left,top,right,bottom=work
        # Cross monitors first, allowing the OS DPI event to settle. Xpeed must
        # not impose its own bounds during normal moves.
        assert u.SetWindowPos(hwnd,None,left+30,top+30,0,0,0x0015)
        time.sleep(.4)
        dpi=u.GetDpiForWindow(hwnd)/96
        width=min(int(800*dpi),right-left-60)
        height=min(int(500*dpi),bottom-top-60)
        assert u.SetWindowPos(hwnd,None,left+30,top+30,width,height,0x0014)
        time.sleep(.7)
        normal=geometry(hwnd)
        assert fits(normal,work),('Move did not fit',normal,work)
        before=(normal[2]-normal[0],normal[3]-normal[1])
        u.ShowWindow(hwnd,3) # maximize
        time.sleep(.5)
        assert u.IsZoomed(hwnd),'Did not maximize'
        u.ShowWindow(hwnd,9) # restore
        time.sleep(.6)
        restored=geometry(hwnd)
        assert fits(restored,work),('Restore spans monitors',restored,work)
        after=(restored[2]-restored[0],restored[3]-restored[1])
        assert abs(before[0]-after[0])<=2 and abs(before[1]-after[1])<=2,('Restore size changed',before,after)
        u.ShowWindow(hwnd,6) # minimize
        time.sleep(.15)
        u.ShowWindow(hwnd,9)
        time.sleep(.4)
        assert fits(geometry(hwnd),work),'Minimize/restore escaped monitor'
        print('Monitor',index,'normal/restored:',before,after,'dpi:',u.GetDpiForWindow(hwnd))
    if len(monitors)>1:
        # Continuous normal-window moves must keep requested coordinates even
        # when the rectangle spans screens. This caught the 0.4.1 snap/clamp bug.
        work=monitors[0]
        left,top,right,bottom=work
        u.SetWindowPos(hwnd,None,left+100,top+100,0,0,0x0015)
        time.sleep(.4)
        u.SetWindowPos(hwnd,None,left+100,top+100,1600,900,0x0014)
        time.sleep(.4)
        baseline=geometry(hwnd)
        baseline_dpi=u.GetDpiForWindow(hwnd)
        for x in [200,50,-100,-300,-500,-700,-900,-1100,-900,-700,-500,-300,-100,100]:
            assert u.SetWindowPos(hwnd,None,x,300,0,0,0x0015)
            time.sleep(.15)
            # Permit the one-time suggested rectangle from the native DPI event,
            # then verify Xpeed does not modify a position on that same monitor.
            assert u.SetWindowPos(hwnd,None,x,300,0,0,0x0015)
            time.sleep(.12)
            actual=geometry(hwnd)
            assert actual[:2]==(x,300),('Normal move snapped back',x,actual)
        u.SetWindowPos(hwnd,None,baseline[0],baseline[1],0,0,0x0015)
        time.sleep(.4)
        final=geometry(hwnd)
        assert u.GetDpiForWindow(hwnd)==baseline_dpi
        assert abs((final[2]-final[0])-(baseline[2]-baseline[0]))<=4,('Round trip shrank width',baseline,final)
        assert abs((final[3]-final[1])-(baseline[3]-baseline[1]))<=4,('Round trip shrank height',baseline,final)
        print('Continuous monitor crossing: no snapping or progressive size loss')
        for work in (monitors[0],monitors[1],monitors[0]):
            u.ShowWindow(hwnd,3)
            time.sleep(.3)
            left,top,right,bottom=work
            assert u.SetWindowPos(hwnd,None,left,top,right-left,bottom-top,0x0014)
            time.sleep(.6)
            assert u.IsZoomed(hwnd),'Moving the maximized test window cleared its state'
            u.ShowWindow(hwnd,9)
            time.sleep(.7)
            assert fits(geometry(hwnd),work),('Restore returned to wrong monitor',geometry(hwnd),work)
            print('Restore after maximized monitor move:',geometry(hwnd),'dpi:',u.GetDpiForWindow(hwnd))
    print('Native monitor/restore checks passed')
finally:
    process.terminate()
    process.wait(timeout=5)
