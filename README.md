# Samsung Trio 500 Fn Fix

삼성 스마트 키보드 트리오 500(EJ-B3400)의 맨 윗줄을 **Fn 없이 F1~F12로** 쓰게 해 주는 Windows 트레이 프로그램입니다.
A Windows tray app that makes the Samsung Smart Keyboard Trio 500 top row act as F1–F12 without holding Fn. ([English](#english))

> 삼성전자와 관련 없는 개인 프로젝트입니다. / Not affiliated with Samsung.

## 문제

트리오 500은 Fn 잠금 기능이 없습니다. 윗줄 키를 그냥 누르면 앱 실행·밝기 같은 특수 기능이 나가고, F1~F12를 쓰려면 매번 Fn을 같이 눌러야 합니다.

## 동작 방식

Fn 없이 누르면 키보드는 F키 대신 HID Consumer Control 신호를 PC로 보냅니다. 이 프로그램은 Raw Input으로 **트리오 500에서 온 신호만** 받아서 해당 F키를 대신 입력합니다. 다른 키보드에는 영향이 없습니다.

| 키 | Fn 없이 보내는 신호 | 바뀌는 키 |
|---|---|---|
| F1 / F2 / F3 | `0x0071` / `0x0072` / `0x0073` | F1 / F2 / F3 |
| F4 / F5 | `0x0302` / `0x0301` | F4 / F5 |
| F6 | `0x0223` (AC Home) | F6 |
| F7 / F8 / F9 | 없음 (블루투스 기기 전환) | — |
| F10 / F11 / F12 | `0x0070` / `0x006F` / `0x006E` | F10 / F11 / F12 |

Fn을 누른 채로 누르면 원래대로 F1~F12가 나가므로, Fn을 누르든 안 누르든 같은 F키가 됩니다.

## 설치

1. [Releases](https://github.com/BertranK/SamsungTrio500FnFix/releases)에서 `Trio500FnFix.exe`를 받습니다.
2. 계속 둘 폴더(예: `%LOCALAPPDATA%\Programs\Trio500FnFix`)에 넣고 실행합니다.
3. 트레이의 파란 **F1** 아이콘을 오른쪽 클릭해서 **Run at Windows startup**을 체크하면 로그인할 때마다 자동으로 실행됩니다.

서명되지 않은 exe라서 처음 실행할 때 SmartScreen이 "Windows의 PC 보호" 경고를 띄울 수 있습니다. **추가 정보 → 실행**을 누르면 됩니다. 소스에서 직접 빌드해도 됩니다(아래 참고).

## 트레이 메뉴

| 메뉴 | 동작 |
|---|---|
| Pause | 잠시 끄기 (체크하면 원래 동작) |
| Run at Windows startup | `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`에 등록/해제 (관리자 권한 불필요) |
| Exit | 종료 |

자동 실행 체크 표시는 등록된 경로가 지금 실행 중인 exe와 같을 때만 켜집니다. exe를 옮겼다면 새 위치에서 다시 체크하세요.

## 제한 사항

- **F7~F9는 계속 Fn이 필요합니다.** 블루투스 기기 전환 키라서 키보드 내부에서 처리되고, Fn 없이는 PC로 아무 신호도 오지 않습니다.
- F6은 Windows에서 브라우저 홈 키로도 처리되므로, 실행 중에는 **모든 키보드의 브라우저 홈 키가 막힙니다.**
- F10/F11은 밝기 신호(`0x0070`/`0x006F`)이기도 해서, 밝기 조절을 지원하는 화면(노트북 등)에서는 밝기도 같이 바뀔 수 있습니다. 데스크톱에서만 확인했습니다.
- 휴대폰·태블릿 등 다른 연결 기기에는 영향이 없습니다. Windows PC에서만 동작합니다.
- 테스트 환경: Windows 11 Pro (26200), 블루투스 LE 연결, `VID 04E8 / PID A064`.

## 빌드

Rust stable (MSVC 타깃)만 있으면 됩니다. 외부 크레이트 없이 Win32 API를 직접 선언했습니다.

```bash
cargo build --release
```

결과물: `target\release\Trio500FnFix.exe` (약 130KB, 메모리 약 2MB)

## 진단 도구

`tools/trio500-keylog.ps1`은 트리오 500이 PC로 보내는 원본 신호(스캔코드, HID 리포트)와, 리매핑 뒤 창이 최종으로 받는 키를 함께 보여 줍니다. 위 표도 이 도구로 만들었습니다. 다른 펌웨어나 지역 모델에서 신호가 다르다면 이 도구로 확인한 뒤 `src/main.rs`의 `FKEY_SCAN` 표를 고치면 됩니다.

```bash
powershell -NoProfile -ExecutionPolicy Bypass -STA -File .\tools\trio500-keylog.ps1
```

## 제거

1. 트레이 메뉴에서 **Run at Windows startup** 체크를 풉니다.
2. **Exit**로 종료합니다.
3. exe 파일(폴더)을 지웁니다.

---

## English

**Problem.** The Trio 500 has no Fn lock. Without Fn, the top row sends special functions; F1–F12 need Fn every time.

**How it works.** Without Fn, the keyboard sends HID Consumer Control usages instead of F-keys (see the table above). This app listens through Raw Input, only for the Trio 500 (`VID 04E8 / PID A064`), and injects the matching F-key. Other keyboards are not affected.

**Install.** Download `Trio500FnFix.exe` from [Releases](https://github.com/BertranK/SamsungTrio500FnFix/releases), put it in a permanent folder, and run it. Right-click the blue **F1** tray icon and check **Run at Windows startup** to start it at sign-in (stored under `HKCU\...\CurrentVersion\Run`, no admin needed). The exe is unsigned, so SmartScreen may warn on first run: choose *More info → Run anyway*.

**Limitations.**
- **F7–F9 still need Fn.** They are the Bluetooth slot keys, handled inside the keyboard, and send nothing to the PC without Fn.
- F6 also arrives as the Browser Home key, so Browser Home is blocked on all keyboards while the app runs.
- F10/F11 are also brightness usages and may change brightness on displays that support it. Only tested on a desktop.
- Tested on Windows 11 Pro (26200) over Bluetooth LE.

**Build.** `cargo build --release` with Rust stable (MSVC target). No external crates.

**Diagnostics.** `tools/trio500-keylog.ps1` logs the raw signals from the keyboard and the key a window finally receives. If your keyboard sends different usages, update the `FKEY_SCAN` table in `src/main.rs`.
