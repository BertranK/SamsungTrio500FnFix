# Trio 500 key diagnostic: logs exactly what each key of the keyboard sends to Windows
# (keyboard scancodes/VKs and raw HID consumer-control reports) via the Raw Input API.
# Run: powershell -NoProfile -ExecutionPolicy Bypass -STA -File .\tools\trio500-keylog.ps1
param(
    [string]$Filter = 'VID&0204E8_PID&A064',
    [string]$LogPath = (Join-Path $PSScriptRoot 'trio500-keylog.txt')
)

$src = @'
using System;
using System.Collections.Generic;
using System.Drawing;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using System.Windows.Forms;

public class TrioKeyLog : Form
{
    const int WM_INPUT = 0x00FF;
    const uint RIDEV_INPUTSINK = 0x00000100;
    const uint RID_INPUT = 0x10000003;
    const uint RIDI_PREPARSEDDATA = 0x20000005;
    const uint RIDI_DEVICENAME = 0x20000007;
    const uint RIDI_DEVICEINFO = 0x2000000b;
    const uint RIM_TYPEKEYBOARD = 1;
    const uint RIM_TYPEHID = 2;
    const int HIDP_STATUS_SUCCESS = 0x00110000;

    [StructLayout(LayoutKind.Sequential)]
    struct RAWINPUTDEVICE { public ushort UsagePage; public ushort Usage; public uint Flags; public IntPtr Target; }
    [StructLayout(LayoutKind.Sequential)]
    struct RAWINPUTDEVICELIST { public IntPtr hDevice; public uint dwType; }
    [StructLayout(LayoutKind.Sequential)]
    struct USAGE_AND_PAGE { public ushort Usage; public ushort UsagePage; }

    [DllImport("user32.dll", SetLastError = true)]
    static extern bool RegisterRawInputDevices(RAWINPUTDEVICE[] devices, uint count, uint size);
    [DllImport("user32.dll", SetLastError = true)]
    static extern uint GetRawInputDeviceList([In, Out] RAWINPUTDEVICELIST[] list, ref uint count, uint size);
    [DllImport("user32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
    static extern uint GetRawInputDeviceInfo(IntPtr device, uint command, IntPtr data, ref uint size);
    [DllImport("user32.dll", SetLastError = true)]
    static extern uint GetRawInputData(IntPtr rawInput, uint command, IntPtr data, ref uint size, uint headerSize);
    [DllImport("hid.dll")]
    static extern int HidP_GetUsagesEx(int reportType, ushort linkCollection, [In, Out] USAGE_AND_PAGE[] list, ref uint length, IntPtr preparsed, byte[] report, uint reportLength);

    class DevInfo { public string Name; public string Short; public uint Type; public ushort Page; public ushort Usage; public IntPtr Preparsed; public bool Match; }

    readonly string filter;
    readonly TextBox box;
    readonly StreamWriter writer;
    readonly Dictionary<IntPtr, DevInfo> devices = new Dictionary<IntPtr, DevInfo>();

    public TrioKeyLog(string filter, string logPath, string help)
    {
        this.filter = filter.ToUpperInvariant();
        Text = "Trio 500 key diagnostic";
        Width = 1000;
        Height = 650;
        TopMost = true;
        Label label = new Label();
        label.Dock = DockStyle.Top;
        label.Height = 80;
        label.Padding = new Padding(8);
        label.Font = new Font("Malgun Gothic", 10);
        label.Text = help;
        box = new TextBox();
        box.Multiline = true;
        box.ReadOnly = true;
        box.ScrollBars = ScrollBars.Both;
        box.WordWrap = false;
        box.Dock = DockStyle.Fill;
        box.Font = new Font("Consolas", 10);
        Controls.Add(box);
        Controls.Add(label);
        writer = new StreamWriter(logPath, false, new UTF8Encoding(false));
        writer.AutoFlush = true;

        // Also log the key this window finally receives (after any remapper), from any source.
        KeyPreview = true;
        KeyDown += delegate(object s, KeyEventArgs e)
        {
            Log(string.Format("WINDOW got {0}", e.KeyCode));
            e.Handled = true;
            e.SuppressKeyPress = true;
        };
        HelpRequested += delegate(object s, HelpEventArgs e) { e.Handled = true; };
    }

    protected override void OnHandleCreated(EventArgs e)
    {
        base.OnHandleCreated(e);
        uint count = 0;
        uint itemSize = (uint)Marshal.SizeOf(typeof(RAWINPUTDEVICELIST));
        GetRawInputDeviceList(null, ref count, itemSize);
        RAWINPUTDEVICELIST[] list = new RAWINPUTDEVICELIST[count];
        GetRawInputDeviceList(list, ref count, itemSize);

        HashSet<uint> wanted = new HashSet<uint>();
        wanted.Add((1u << 16) | 6u);
        foreach (RAWINPUTDEVICELIST item in list)
        {
            DevInfo d = Lookup(item.hDevice);
            if (d == null || !d.Match) continue;
            Log(string.Format("device {0} type={1} page=0x{2:X4} usage=0x{3:X4} {4}", d.Short, d.Type, d.Page, d.Usage, d.Name));
            if (d.Type == RIM_TYPEHID) wanted.Add(((uint)d.Page << 16) | d.Usage);
        }

        List<RAWINPUTDEVICE> regs = new List<RAWINPUTDEVICE>();
        foreach (uint key in wanted)
        {
            RAWINPUTDEVICE r = new RAWINPUTDEVICE();
            r.UsagePage = (ushort)(key >> 16);
            r.Usage = (ushort)(key & 0xFFFF);
            r.Flags = RIDEV_INPUTSINK;
            r.Target = Handle;
            regs.Add(r);
        }
        bool ok = RegisterRawInputDevices(regs.ToArray(), (uint)regs.Count, (uint)Marshal.SizeOf(typeof(RAWINPUTDEVICE)));
        Log(string.Format("registered {0} collections: {1} (err {2})", regs.Count, ok, ok ? 0 : Marshal.GetLastWin32Error()));
        Log("---- press keys now ----");
    }

    DevInfo Lookup(IntPtr h)
    {
        DevInfo d;
        if (devices.TryGetValue(h, out d)) return d;

        uint size = 0;
        GetRawInputDeviceInfo(h, RIDI_DEVICENAME, IntPtr.Zero, ref size);
        if (size == 0) return null;
        IntPtr nameBuf = Marshal.AllocHGlobal((int)size * 2);
        string name;
        try
        {
            GetRawInputDeviceInfo(h, RIDI_DEVICENAME, nameBuf, ref size);
            name = Marshal.PtrToStringUni(nameBuf);
        }
        finally { Marshal.FreeHGlobal(nameBuf); }

        d = new DevInfo();
        d.Name = name;
        d.Match = name.ToUpperInvariant().Contains(filter);
        int col = name.IndexOf("&Col", StringComparison.OrdinalIgnoreCase);
        d.Short = col >= 0 ? name.Substring(col + 1, 5).ToUpperInvariant() : "DEV";

        uint infoSize = 32;
        IntPtr info = Marshal.AllocHGlobal(32);
        try
        {
            Marshal.WriteInt32(info, 0, 32);
            GetRawInputDeviceInfo(h, RIDI_DEVICEINFO, info, ref infoSize);
            d.Type = (uint)Marshal.ReadInt32(info, 4);
            if (d.Type == RIM_TYPEHID)
            {
                d.Page = (ushort)Marshal.ReadInt16(info, 20);
                d.Usage = (ushort)Marshal.ReadInt16(info, 22);
            }
            else if (d.Type == RIM_TYPEKEYBOARD) { d.Page = 1; d.Usage = 6; }
        }
        finally { Marshal.FreeHGlobal(info); }

        if (d.Match && d.Type == RIM_TYPEHID)
        {
            uint ppSize = 0;
            GetRawInputDeviceInfo(h, RIDI_PREPARSEDDATA, IntPtr.Zero, ref ppSize);
            if (ppSize > 0)
            {
                d.Preparsed = Marshal.AllocHGlobal((int)ppSize);
                GetRawInputDeviceInfo(h, RIDI_PREPARSEDDATA, d.Preparsed, ref ppSize);
            }
        }
        devices[h] = d;
        return d;
    }

    protected override void WndProc(ref Message m)
    {
        if (m.Msg == WM_INPUT) HandleInput(m.LParam);
        base.WndProc(ref m);
    }

    void HandleInput(IntPtr hRaw)
    {
        uint headerSize = (uint)(8 + 2 * IntPtr.Size);
        uint size = 0;
        GetRawInputData(hRaw, RID_INPUT, IntPtr.Zero, ref size, headerSize);
        if (size == 0) return;
        IntPtr buf = Marshal.AllocHGlobal((int)size);
        try
        {
            if (GetRawInputData(hRaw, RID_INPUT, buf, ref size, headerSize) != size) return;
            uint type = (uint)Marshal.ReadInt32(buf, 0);
            DevInfo d = Lookup(Marshal.ReadIntPtr(buf, 8));
            if (d == null || !d.Match) return;
            int off = (int)headerSize;
            if (type == RIM_TYPEKEYBOARD)
            {
                ushort make = (ushort)Marshal.ReadInt16(buf, off);
                ushort flags = (ushort)Marshal.ReadInt16(buf, off + 2);
                ushort vk = (ushort)Marshal.ReadInt16(buf, off + 6);
                string ext = (flags & 2) != 0 ? "E0 " : ((flags & 4) != 0 ? "E1 " : "");
                Log(string.Format("{0} KEY {1,-4} vk=0x{2:X2} {3,-14} sc={4}0x{5:X2}",
                    d.Short, (flags & 1) != 0 ? "up" : "down", vk, (Keys)vk, ext, make));
            }
            else if (type == RIM_TYPEHID)
            {
                int sizeHid = Marshal.ReadInt32(buf, off);
                int count = Marshal.ReadInt32(buf, off + 4);
                for (int i = 0; i < count; i++)
                {
                    byte[] report = new byte[sizeHid];
                    Marshal.Copy(IntPtr.Add(buf, off + 8 + i * sizeHid), report, 0, sizeHid);
                    Log(string.Format("{0} HID page=0x{1:X4} raw=[{2}] usages=[{3}]",
                        d.Short, d.Page, BitConverter.ToString(report), Decode(d, report)));
                }
            }
        }
        finally { Marshal.FreeHGlobal(buf); }
    }

    string Decode(DevInfo d, byte[] report)
    {
        if (d.Preparsed == IntPtr.Zero) return "?";
        USAGE_AND_PAGE[] list = new USAGE_AND_PAGE[32];
        uint len = (uint)list.Length;
        int st = HidP_GetUsagesEx(0, 0, list, ref len, d.Preparsed, report, (uint)report.Length);
        if (st != HIDP_STATUS_SUCCESS) return string.Format("status 0x{0:X8}", st);
        StringBuilder sb = new StringBuilder();
        for (int i = 0; i < len; i++)
        {
            if (sb.Length > 0) sb.Append(' ');
            sb.AppendFormat("{0:X2}:{1:X4}", list[i].UsagePage, list[i].Usage);
        }
        return sb.ToString();
    }

    void Log(string line)
    {
        line = DateTime.Now.ToString("HH:mm:ss.fff") + "  " + line;
        writer.WriteLine(line);
        box.AppendText(line + Environment.NewLine);
    }
}
'@

$help = @"
이 창을 클릭한 뒤 트리오 500에서 누르고 싶은 키를 하나씩 누르세요. 'HID'/'KEY' 줄은 키보드가 보낸 원본 신호, 'WINDOW got' 줄은 이 창이 최종으로 받은 키입니다.
F7~F9는 Fn 없이 누르면 키보드가 다른 기기로 전환될 수 있습니다. 다 끝나면 이 창을 닫으세요.
"@

Add-Type -TypeDefinition $src -ReferencedAssemblies System.Windows.Forms, System.Drawing
[System.Windows.Forms.Application]::EnableVisualStyles()
[System.Windows.Forms.Application]::Run([TrioKeyLog]::new($Filter, $LogPath, $help))
