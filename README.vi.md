<div align="center">

# PHPev

**Môi trường PHP & Laravel local cho Windows: nhanh, nhẹ, mã nguồn mở.**

Cài PHP, Composer và MariaDB trong vài giây, đổi version theo từng project, và có ngay domain
`http://<project>.localhost` (kèm HTTPS nếu cần) cho mỗi thư mục. Tất cả chỉ gồm vài file
`.exe` native, dùng hằng ngày không cần quyền admin.

[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#giấy-phép)
![Platform: Windows 10/11 x64](https://img.shields.io/badge/platform-Windows%2010%20%7C%2011%20x64-0078D6.svg)
![Written in Rust](https://img.shields.io/badge/written%20in-Rust-orange.svg)
![Status: pre-release](https://img.shields.io/badge/status-pre--release-yellow.svg)

[English](README.md) · Tiếng Việt

</div>

---

## Mục lục

- [Vì sao có PHPev](#vì-sao-có-phpev)
- [Tính năng](#tính-năng)
- [Yêu cầu](#yêu-cầu)
- [Cài đặt](#cài-đặt)
- [Bắt đầu nhanh](#bắt-đầu-nhanh)
- [Hướng dẫn sử dụng](#hướng-dẫn-sử-dụng)
  - [Site và domain](#site-và-domain)
  - [Import project có sẵn](#import-project-có-sẵn)
  - [Version PHP](#version-php)
  - [MariaDB](#mariadb)
  - [Composer](#composer)
  - [HTTPS](#https)
  - [Dashboard và icon tray](#dashboard-và-icon-tray)
  - [Dịch vụ chạy nền](#dịch-vụ-chạy-nền)
- [Danh sách lệnh](#danh-sách-lệnh)
- [Cấu hình](#cấu-hình)
- [Cách hoạt động](#cách-hoạt-động)
- [Hiệu năng](#hiệu-năng)
- [Bảo mật](#bảo-mật)
- [Xử lý sự cố](#xử-lý-sự-cố)
- [Gỡ cài đặt](#gỡ-cài-đặt)
- [Build từ mã nguồn](#build-từ-mã-nguồn)
- [Lộ trình](#lộ-trình)
- [Đóng góp](#đóng-góp)
- [Giấy phép](#giấy-phép)
- [Lời cảm ơn](#lời-cảm-ơn)

---

## Vì sao có PHPev

Các công cụ như [Laravel Herd](https://herd.laravel.com/windows) giúp lập trình PHP trên Windows dễ
chịu hơn nhiều, nhưng không phải mã nguồn mở. Các bộ kiểu XAMPP thì chỉ có một version PHP cố định
kèm cả một Apache. PHPev hướng tới trải nghiệm như Herd, với ba ưu tiên:

- **Mã nguồn mở.** Giấy phép kép MIT / Apache-2.0: đọc, fork, phân phối tùy ý.
- **Nhẹ.** Dịch vụ nền dùng khoảng **2 MB RAM** khi rảnh. Không Electron, không WebView2, không kèm
  web server riêng: router HTTP nằm ngay trong dịch vụ.
- **Nhanh.** Router có sẵn chạy PHP qua một pool worker `php-cgi`, giữ kết nối FastCGI (keep-alive).
  Trong các phép đo, nó ngang hoặc nhanh hơn nginx + php-cgi (xem [Hiệu năng](#hiệu-năng)).

## Tính năng

- **Một lệnh để project chạy được.** `phpev import` nhận diện Laravel hoặc PHP thuần, chọn version
  PHP theo `composer.json`, bật extension còn thiếu, chạy `composer install`, tạo `.env`, tạo
  database và gán domain cho project.
- **Nhiều version PHP cùng lúc** (7.4 – 8.5, bản chính thức từ windows.php.net), chọn theo từng
  project bằng `.php-version`, theo site, hoặc mặc định chung. Lệnh `php` và `composer` trong mọi
  terminal tự dùng đúng version của thư mục hiện tại.
- **MariaDB** (10.6 – 13.0, bản chính thức), mỗi major version một thư mục dữ liệu riêng, chỉ chạy
  khi cần và tự bật lại sau khi khởi động lại máy.
- **Composer** được cài và cập nhật sẵn, chạy bằng đúng version PHP của project.
- **Domain tự động**: `park` một thư mục thì mọi thư mục con thành `http://<tên>.localhost`;
  `link` cho từng project. Domain `.localhost` không cần sửa file hosts, không cần quyền admin.
- **HTTPS theo từng site**, bằng một CA nội bộ chỉ được phép cấp cert cho các TLD local.
- **Dashboard web** (mở trong cửa sổ app riêng): quản lý site, version PHP và extension, dịch vụ;
  **biểu đồ hiệu năng realtime** (req/s, độ trễ p50/p95/p99, route chậm nhất, CPU/RAM từng process)
  và **xem log realtime** (PHPev, PHP, MariaDB và `storage/logs` của từng app Laravel).
- **Icon tray** để mở dashboard và site, bật/tắt MariaDB, thoát.
- **An toàn mặc định**: mọi file tải về đều được kiểm tra SHA-256; chỉ xin quyền admin cho các tính
  năng tùy chọn; thao tác có tác dụng phụ đều hỏi trước khi chạy.

## Yêu cầu

| | |
|---|---|
| Hệ điều hành | Windows 10 (1809 trở lên) hoặc Windows 11, x64 |
| Dung lượng | ~90 MB mỗi version PHP, ~60 MB mỗi version MariaDB (chưa kể database) |
| Internet | Chỉ cần lần đầu để tải PHP, MariaDB, Composer |
| Quyền admin | Không cần khi dùng bình thường (các ngoại lệ tùy chọn ở mục [Bảo mật](#bảo-mật)) |
| Port trống | 80 và 443 (site), 3306 (MariaDB), 7600 (dashboard); đều đổi được |

Không cần cài gì trước: các file exe được build với C runtime tĩnh, còn Visual C++ runtime mà PHP
cần thì PHPev tự cài nếu máy chưa có. Dashboard mở bằng Microsoft Edge (có sẵn trên Windows);
không có Edge thì mở bằng trình duyệt mặc định.

## Cài đặt

> **Bản chưa phát hành:** PHPev chưa có file build sẵn. Cho tới khi bản đầu tiên xuất hiện ở
> [trang Releases](https://github.com/duy-lee/PHPev/releases), hãy [build từ mã nguồn](#build-từ-mã-nguồn).
> Các bước dưới đây giống nhau cho cả hai cách.

1. Tải `phpev-<version>-windows-x64.zip` từ [Releases](https://github.com/duy-lee/PHPev/releases)
   và giải nén ở đâu cũng được (ví dụ thư mục Downloads).
2. Mở terminal trong thư mục đó và chạy:

   ```powershell
   .\phpev.exe setup
   ```

   `setup` tạo thư mục home của PHPev (`%USERPROFILE%\.phpev`), copy các file exe vào
   `%USERPROFILE%\.phpev\shims`, thêm đúng một thư mục đó vào PATH **của user**, rồi kiểm tra máy.
   Sau bước này có thể xóa thư mục vừa giải nén.
3. **Mở terminal mới** để nhận PATH mới, rồi kiểm tra lại:

   ```powershell
   phpev doctor
   ```

> Nếu tên user Windows có dấu (ví dụ `Mê Ga Lô Đôn`), PHP không nạp được `php.ini` từ đường dẫn đó.
> `setup` sẽ phát hiện và đề xuất dùng `C:\PHPev` làm home; bạn cũng có thể tự chọn bằng
> `phpev setup --home D:\PHPev`.

## Bắt đầu nhanh

```powershell
# Cài một version PHP (version đầu tiên sẽ thành mặc định)
phpev php install 8.4

# Mọi project trong C:\code chạy ở http://<tên thư mục>.localhost
phpev park C:\code

# Hoặc cho một project có sẵn chạy được từ đầu tới cuối
cd C:\code\my-laravel-app
phpev import

# Mở dashboard
phpev dashboard
```

Giờ `http://my-laravel-app.localhost` đã chạy app. Dịch vụ nền tự khởi động ở lần đầu một lệnh cần đến nó.

## Hướng dẫn sử dụng

### Site và domain

PHPev phục vụ site qua router riêng trên port 80/443. Document root là thư mục `public` nếu có
`public/index.php` (Laravel, Symfony…), ngược lại là chính thư mục project.

```powershell
phpev park C:\code          # mọi thư mục con của C:\code thành <tên>.localhost
phpev unpark C:\code

cd C:\code\shop
phpev link                  # thư mục này thành shop.localhost
phpev link store --php 8.3  # ...thành store.localhost, cố định PHP 8.3
phpev unlink

phpev sites                 # liệt kê site: URL, version PHP, thư mục
```

Tên site lấy từ tên thư mục: chữ thường, bỏ dấu, ký tự khác đổi thành `-` (`Dự án Mới` → `du-an-moi`).

**Vì sao dùng `.localhost`?** Trình duyệt, `curl` và extension curl của PHP (Laravel `Http::`,
Guzzle) tự trỏ `*.localhost` về 127.0.0.1, nên không cần file hosts, không cần admin. Nhưng bộ
phân giải tên của Windows thì không, nên code dùng PHP stream (`file_get_contents`, transport native
của Symfony HttpClient) hoặc PowerShell sẽ không gọi được `*.localhost`. Khi đó ghi các site vào
file hosts một lần (một lần UAC):

```powershell
phpev hosts sync
```

Cũng có thể dùng TLD khác như `.test`; khi đó file hosts tự cập nhật mỗi khi danh sách site thay đổi:

```powershell
phpev tld test       # site thành http://<tên>.test
phpev tld localhost  # quay về mặc định (các dòng hosts của PHPev được gỡ)
```

### Import project có sẵn

```powershell
cd C:\code\my-app
phpev import                 # hỏi trước mỗi bước có thay đổi
phpev import --yes           # đồng ý mọi bước (trừ migrate database)
phpev import --yes --migrate # ...và chạy `php artisan migrate`
```

`import` chạy các bước sau, chạy lại nhiều lần vẫn an toàn:

1. **Nhận diện** Laravel (`artisan` + `laravel/framework`), project Composer, hoặc thư mục PHP thuần.
2. **PHP**: dùng `.php-version` nếu có; nếu không thì lấy version cao nhất thỏa `require.php`
   (và `config.platform.php`). Chưa có version phù hợp thì đề xuất cài.
3. **Extension**: so các yêu cầu `ext-*` trong `composer.json`/`composer.lock` với bản PHP, đề
   xuất bật extension còn thiếu. Extension chỉ có trên PECL (redis, imagick…) thì báo lại.
4. **Composer**: cài Composer nếu chưa có, rồi chạy `composer install` bằng version PHP đã chọn.
5. **Domain**: link project thành `<thư mục>.localhost` (hoặc theo `--name`).
6. **`.env`** (Laravel): tạo từ `.env.example`, điền `APP_URL`, kết nối MariaDB và `APP_KEY` mới.
   `.env` đã có thì **không bao giờ bị sửa**, chỉ báo những chỗ lệch.
7. **Database**: tạo file SQLite, hoặc cài/bật MariaDB rồi tạo database.
8. **Front-end**: nhắc chạy `npm install && npm run build` nếu asset chưa được build.

Version PHP đã chọn được lưu trong config của PHPev, không thêm file nào vào repo của bạn.

`import` có thể thêm exclusion Microsoft Defender cho thư mục project để Composer và Artisan chạy
nhanh hơn. Đổi lại, mã độc ghi vào thư mục đó sẽ không bị quét khi ghi, nên mặc định **tắt**, luôn
hỏi trước, và `--yes` không tự bật. Dùng `--defender-exclude` để bật mà không cần hỏi.

### Version PHP

```powershell
phpev php list --available   # version đã cài và có thể tải
phpev php install 8.3        # bản 8.3.x mới nhất (hoặc version cụ thể: 8.3.36)
phpev php use 8.4            # version mặc định khi ở ngoài project
phpev php which              # ở đây `php` sẽ chạy version nào, vì sao
phpev php ini 8.4            # đường dẫn php.ini của version đó
```

Lệnh `php` và `composer` trong mọi terminal là các shim nhỏ, chọn version theo thứ tự:

1. file `.php-version` ở thư mục hiện tại hoặc thư mục cha gần nhất (`8.4`, `8.4.27`, `php-8.4`);
2. site chứa thư mục hiện tại (`phpev link --php`, hoặc version do `import` chọn);
3. version mặc định.

Process con kế thừa lựa chọn này, nên `php artisan serve`, queue worker và script của Composer đều
chạy cùng version. Đặt `PHPEV_SHIM_DEBUG=1` để shim in ra version đã chọn và lý do.

Mỗi version có `php.ini` riêng do PHPev sinh ra, bật sẵn OPcache và cấu hình CA bundle để PHP gọi
HTTPS được ngay. Bật/tắt extension ở trang PHP của dashboard hoặc sửa trực tiếp `php.ini`.

### MariaDB

```powershell
phpev db list --available
phpev db install 11.4   # bản 11.4.x mới nhất
phpev db start          # được ghi nhớ: lần sau PHPev khởi động thì MariaDB bật lại
phpev db stop
phpev db use 10.11      # đổi version; mỗi major giữ database riêng
```

MariaDB lắng nghe ở `127.0.0.1:3306`, user `root`, **không mật khẩu**. Nó chỉ dành cho phát triển
local và máy khác không truy cập được.

### Composer

```powershell
phpev composer install       # tải composer.phar nếu chưa có
phpev composer self-update   # cập nhật lên bản stable mới nhất
composer --version           # chạy bằng version PHP của thư mục hiện tại
```

### HTTPS

```powershell
cd C:\code\shop
phpev secure          # https://shop.localhost; http:// tự chuyển sang https
phpev unsecure
phpev ca status
phpev ca uninstall    # gỡ CA khỏi Windows và PHP, xóa key, tắt HTTPS cho mọi site
```

Lần `phpev secure` đầu tiên tạo một CA nội bộ và thêm vào kho chứng chỉ gốc tin cậy **của user hiện
tại** (Windows hỏi xác nhận một lần). CA có *name constraints*: nó chỉ cấp được cert cho `localhost`,
`test`, `local` và TLD bạn cấu hình, nên kể cả khi lộ key cũng không thể giả mạo website thật. Cert
của từng site được tạo trong RAM khi cần. CA cũng được thêm vào CA bundle mà PHP dùng.

### Dashboard và icon tray

```powershell
phpev dashboard             # mở trong cửa sổ app riêng (Microsoft Edge)
phpev dashboard --browser   # mở trong tab trình duyệt thường
phpev tray off              # ẩn icon tray (phpev tray on để hiện lại)
```

Dashboard chạy ở `http://127.0.0.1:7600`, gồm các trang:

| Trang | Làm được gì |
|---|---|
| **Overview** | Trạng thái dịch vụ, uptime và RAM, router/HTTPS, MariaDB, pool PHP, cảnh báo của `doctor` |
| **Sites** | Link/unlink, bật/tắt HTTPS, đổi version PHP, mở site, mở thư mục hoặc terminal |
| **PHP** | Cài version (có tiến độ), đặt mặc định, bật/tắt extension, restart worker |
| **Services** | Bật/tắt MariaDB, đổi hoặc cài version |
| **Performance** | Realtime: req/s, độ trễ p50/p95, histogram độ trễ, mã trạng thái, route chậm nhất, CPU/RAM từng process; lọc theo site, có/không tính file tĩnh |
| **Logs** | Theo dõi realtime log của PHPev, PHP, MariaDB và Laravel (`storage/logs/*.log`), có ô lọc |

Dữ liệu hiệu năng chỉ nằm trong RAM (số liệu gộp theo giây của 10 phút gần nhất và 2 000 request gần
nhất). Query string không bao giờ được lưu, và ID trong URL được gom nhóm (`/users/42` → `/users/{n}`).

**Icon tray**: click trái để mở dashboard. Menu chuột phải gồm *Open dashboard*, *Sites*,
*Start/Stop MariaDB*, *Start with Windows* và *Exit PHPev*. Cửa sổ dashboard tự đóng khi PHPev dừng.

### Dịch vụ chạy nền

`phpev-daemon.exe` chạy router, các worker PHP, MariaDB và dashboard. Nó tự khởi động khi một lệnh
cần đến, và vẫn chạy khi bạn đóng terminal.

```powershell
phpev status         # đang chạy gì, port, PID và RAM
phpev start
phpev stop           # dừng tất cả (MariaDB được tắt êm)
phpev autostart on   # tự chạy khi đăng nhập Windows (không cần admin)
phpev autostart off
```

Worker PHP khởi động ở request đầu tiên: mỗi version PHP 2 worker, tăng tới 8 khi tải cao, giảm lại
sau 60 giây rảnh và dừng hẳn sau 10 phút không có request. Process bị crash được tự khởi động lại
(có giãn cách thời gian). Mọi process con gắn với dịch vụ nên không còn process mồ côi kể cả khi dịch
vụ bị kill; MariaDB được tắt êm khi Windows log off hoặc shutdown.

## Danh sách lệnh

| Lệnh | Mô tả |
|---|---|
| `phpev setup [--home DIR]` | Tạo home của PHPev, đưa shim vào PATH, kiểm tra máy |
| `phpev doctor [--fix-path] [--defender-exclude]` | Kiểm tra sự cố (thứ tự PATH, port, runtime, CA…) |
| `phpev php list\|install\|use\|ini\|which\|warm` | Quản lý version PHP |
| `phpev db list\|install\|start\|stop\|use` | Quản lý MariaDB |
| `phpev composer install\|self-update` | Quản lý Composer |
| `phpev start\|stop\|status` | Điều khiển dịch vụ nền |
| `phpev autostart on\|off` | Chạy dịch vụ khi đăng nhập |
| `phpev dashboard [--browser]` | Mở dashboard |
| `phpev tray on\|off` | Hiện/ẩn icon tray |
| `phpev park [DIR]` / `unpark [DIR]` | Phục vụ mọi thư mục con của một thư mục |
| `phpev link [NAME] [--php VER]` / `unlink [NAME]` | Phục vụ một thư mục |
| `phpev sites` | Liệt kê site |
| `phpev import [DIR] [--name N] [--yes] [--migrate] [--defender-exclude]` | Cho project có sẵn chạy được |
| `phpev secure [NAME]` / `unsecure [NAME]` | HTTPS cho một site |
| `phpev ca status\|uninstall` | Quản lý CA nội bộ |
| `phpev tld [TLD]` | Xem hoặc đổi TLD (mặc định `localhost`) |
| `phpev hosts sync` | Ghi mọi site vào file hosts của Windows |

Chạy `phpev <lệnh> --help` để xem đầy đủ tùy chọn.

## Cấu hình

Cấu hình nằm ở `%USERPROFILE%\.phpev\config.toml`. CLI và dashboard tự cập nhật file này, nhưng đây
là TOML thuần nên sửa tay cũng an toàn. Thay đổi về site có hiệu lực ở request kế tiếp; đổi port thì
cần `phpev stop` rồi `phpev start`.

```toml
php = "8.4"           # version PHP mặc định
mariadb = "11.4"      # major version MariaDB đang dùng
tld = "localhost"
parked = ['C:\code']
# tray = false        # ẩn icon tray

[ports]
http = 80
https = 443
mariadb = 3306
dashboard = 7600

[[sites]]
name = "shop"
path = 'C:\code\shop'
php = "8.3"           # tùy chọn: cố định version PHP
secure = true         # tùy chọn: HTTPS
```

Biến môi trường:

| Biến | Tác dụng |
|---|---|
| `PHPEV_HOME` | Dùng thư mục home khác (do `phpev setup --home` đặt) |
| `PHPEV_SHIM_DEBUG=1` | Shim `php`/`composer` in ra version đã chọn và lý do |
| `PHPEV_NO_METRICS=1` | Tắt ghi số liệu request trong dịch vụ (dùng khi benchmark) |

Thư mục:

```
%USERPROFILE%\.phpev\
├── shims\          thư mục duy nhất trong PATH: php.exe, composer.exe, phpev*.exe
├── bin\            php\<version>\, mariadb\<version>\, composer\, cacert.pem
├── data\mariadb\   database, mỗi major version một thư mục
├── certs\          CA nội bộ
├── logs\           daemon.log, php-<version>.log, mariadb-<version>.err
├── cache\          file đang tải
└── config.toml
```

## Cách hoạt động

```
                 ┌──────────────────────── phpev-daemon.exe (≈2 MB RAM khi rảnh) ─────────────────────┐
 trình duyệt ─80/443─▶ router HTTP/HTTPS ── file tĩnh                                                  │
                 │        │                                                                            │
                 │        └── FastCGI (keep-alive) ──▶ pool php-cgi cho mỗi version PHP (2–8 worker)   │
                 │                                                                                     │
 dashboard ─7600─▶ API v1 (JSON + SSE) ── metrics, log, job          MariaDB (được giám sát)           │
 icon tray ──────▶ cửa sổ ẩn (đồng thời xử lý Windows shutdown)                                        │
                 └──────────────▲──────────────────────────────────────────────────────────────────────┘
                                │ named pipe
 terminal: phpev.exe ───────────┘        shim php.exe / composer.exe ──▶ php.exe thật đúng version
```

- **`phpev.exe`**: dòng lệnh, giao tiếp với dịch vụ qua named pipe riêng của từng user.
- **`phpev-daemon.exe`**: dịch vụ nền viết bằng Rust/tokio, gồm router (hyper, rustls), giám sát
  process, dashboard và icon tray. Mọi process con chạy trong một Job Object kill-on-close.
- **`php.exe` / `composer.exe`**: shim rất nhỏ, chọn version PHP rồi chạy bản thật.
- **`phpev-elevate.exe`**: chạy vài thao tác cần quyền admin, mỗi lần một hộp thoại UAC.

Dashboard là một app Preact nhỏ nhúng trong binary của dịch vụ. Nó chỉ dùng API đã công bố
(`/api/v1`), nên có thể thay bằng giao diện khác.

## Hiệu năng

Đo trên máy phát triển (Windows 11, PHP 8.4, 8 worker PHP, 8 kết nối đồng thời):

| Kịch bản | PHPev | nginx 1.30 + php-cgi |
|---|---|---|
| `ping.php`, tuần tự, TTFB trung vị | 1,4 ms | 1,3 ms |
| Trang welcome Laravel, tuần tự, TTFB trung vị | 42,0 ms | 43,1 ms |
| `ping.php`, 8 kết nối | 5 700–6 900 req/s, 0 lỗi | 4 400–7 600 req/s, 32–57 lỗi, có request treo 60 s |

Số liệu khác: RAM dịch vụ khi rảnh **~2 MB** (private working set, như cột Memory của Task
Manager); biểu đồ realtime chỉ tốn **0,07%** CPU khi đang mở; chi phí ghi số liệu request nhỏ hơn
mức nhiễu khi đo; shim `php` tốn thêm khoảng một lần tạo process (~11 ms) mỗi lần gọi; mở dashboard
từ tray mất khoảng **0,6 s**.

Đây là số đo trên một máy. Mời bạn mở issue chia sẻ số đo của mình.

## Bảo mật

- **Mọi thứ chỉ lắng nghe trên loopback** (`127.0.0.1` / `::1`).
- **Dashboard**: mỗi lần dịch vụ khởi động sinh một token ngẫu nhiên mới, đổi lấy cookie `HttpOnly`,
  `SameSite=Strict`. Có kiểm tra `Host` và `Origin`, yêu cầu JSON cho thao tác thay đổi (chống CSRF),
  gửi Content-Security-Policy chặt. Không có web shell: dashboard chỉ gọi được một tập API cố định.
- **File tải về** (PHP, MariaDB, Composer) lấy từ nguồn chính thức và được kiểm tra SHA-256 trước khi dùng.
- **Quyền admin** chỉ được xin (mỗi lần một UAC) cho các thao tác tùy chọn: ghi file hosts,
  exclusion Defender, đưa shim lên trước một PHP khác trong System PATH, cài Visual C++ runtime.
- **CA nội bộ** bị giới hạn bằng name constraints ở các TLD local và chỉ được tin cho user hiện tại.
- **MariaDB** dùng `root` không mật khẩu và chỉ truy cập được từ máy này. Đừng dùng PHPev để host
  bất cứ thứ gì ra mạng.

Phát hiện lỗ hổng? Vui lòng báo riêng qua
[GitHub Security Advisories](https://github.com/duy-lee/PHPev/security/advisories/new), đừng mở issue công khai.

## Xử lý sự cố

**`php -v` ra một PHP khác (XAMPP, Laragon…).**
Windows ghép *System PATH* trước *User PATH*, nên PHP trong System PATH được ưu tiên. `phpev doctor`
cho biết đó là PHP nào. Sửa bằng `phpev doctor --fix-path` (đưa PHPev lên trước, cần admin) hoặc gỡ
PHP cũ khỏi System PATH, rồi mở terminal mới.
Lưu ý `where php` chạy khác nhau: trong `cmd` nó là `where.exe`, còn trong PowerShell `where` là
alias của `Where-Object` (hãy dùng `where.exe php`).

**Port 80, 443 hoặc 3306 bị chiếm.**
`phpev status` và `phpev doctor` cho biết chương trình nào đang giữ port (thường là Apache/MySQL của
XAMPP, IIS hoặc Skype). Tắt chương trình đó, hoặc đổi port trong mục `[ports]` của `config.toml`.

**`file_get_contents('http://app.localhost')` lỗi trong PHP.**
Windows không phân giải `*.localhost`; chạy `phpev hosts sync` một lần.

**Trình duyệt cảnh báo chứng chỉ.**
Chạy `phpev ca status`. Nếu CA chưa được tin, chạy lại `phpev secure` rồi khởi động lại trình duyệt.
Firefox dùng kho chứng chỉ riêng: bật `security.enterprise_roots.enabled` trong `about:config`.

**Composer hoặc Artisan chậm.**
Nguyên nhân thường là antivirus quét realtime. `phpev doctor --defender-exclude` loại trừ home của
PHPev, `phpev import --defender-exclude` loại trừ một project. Đặt code trên
[Dev Drive](https://learn.microsoft.com/windows/dev-drive/) là lựa chọn an toàn hơn.

**Lỗi khác.**
Xem `%USERPROFILE%\.phpev\logs\daemon.log` hoặc trang Logs của dashboard, rồi
[mở issue](https://github.com/duy-lee/PHPev/issues) kèm kết quả của `phpev doctor` và `phpev status`.

## Gỡ cài đặt

```powershell
phpev autostart off
phpev ca uninstall     # chỉ khi đã dùng HTTPS
phpev stop
```

Sau đó gỡ `%USERPROFILE%\.phpev\shims` khỏi PATH của user (*Settings → System → About → Advanced
system settings → Environment Variables*). Nếu đã dùng `phpev hosts sync` hoặc TLD khác, hãy chạy
`phpev tld localhost` trước để gỡ các dòng hosts của PHPev. Cuối cùng xóa thư mục
`%USERPROFILE%\.phpev`. **Thao tác này cũng xóa database MariaDB của bạn**, hãy sao lưu trước nếu cần.

## Build từ mã nguồn

Cần: [Rust](https://rustup.rs/) (stable) và Visual Studio 2022 Build Tools với workload
*Desktop development with C++* (MSVC linker, Windows SDK và `rc.exe`).

```powershell
git clone https://github.com/duy-lee/PHPev.git
cd PHPev
cargo build --release
.\target\release\phpev.exe setup
```

Bản release link C runtime tĩnh (xem `.cargo/config.toml`), nên chạy được trên Windows mới cài.

Không có Visual Studio thì có thể build bằng toolchain GNU với [WinLibs](https://winlibs.com/)
MinGW-w64 (MSVCRT) trong PATH:

```powershell
rustup toolchain install stable-x86_64-pc-windows-gnu
winget install BrechtSanders.WinLibs.POSIX.MSVCRT
cargo +stable-x86_64-pc-windows-gnu build --release
```

Cấu trúc project:

| Crate | Kết quả | Vai trò |
|---|---|---|
| `crates/core` | thư viện | Config, tải file, PHP/MariaDB/Composer, site, CA, kiểu dữ liệu API, các thao tác dùng chung |
| `crates/cli` | `phpev.exe` | Dòng lệnh |
| `crates/daemon` | `phpev-daemon.exe` | Dịch vụ nền, router, dashboard (`crates/daemon/dashboard`), tray |
| `crates/shim` | `phpev-shim.exe` | Được cài thành `php.exe` và `composer.exe` |
| `crates/elevate` | `phpev-elevate.exe` | Các thao tác cần quyền admin |

Kiểm tra trước mỗi commit:

```powershell
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

## Lộ trình

Phase 1 (mọi thứ ở trên) đã hoàn tất. Dự kiến tiếp theo:

- [ ] CI và phát hành file build sẵn
- [ ] Bật/tắt Xdebug theo version PHP
- [ ] Backend nginx hoặc Apache tùy chọn cho từng site (cho `.htaccess` và rewrite rule kiểu nginx)
- [ ] Thêm dịch vụ: mail catcher, Redis, queue worker, quản lý version Node.js

Mời bạn góp ý và bình chọn ở [issues](https://github.com/duy-lee/PHPev/issues).

## Đóng góp

Mọi đóng góp đều được hoan nghênh, từ báo lỗi tới code.

1. Với thay đổi lớn, hãy mở issue trước để thống nhất cách làm.
2. Fork repo và tạo branch mới.
3. Giữ các bước kiểm tra ở mục [Build từ mã nguồn](#build-từ-mã-nguồn) luôn đạt, và thêm test cho logic mới.
4. Mở pull request, mô tả thay đổi gì và đã test thế nào.

PHPev chú trọng giữ nhỏ và nhanh: nếu thay đổi có thể ảnh hưởng tới RAM, kích thước hoặc độ trễ,
hãy nêu rõ trong pull request.

## Giấy phép

PHPev dùng giấy phép kép, bạn chọn một trong hai:

- [MIT License](LICENSE-MIT)
- [Apache License, Version 2.0](LICENSE-APACHE)

Trừ khi bạn ghi rõ khác đi, mọi đóng góp bạn chủ động gửi để đưa vào dự án, theo định nghĩa của
giấy phép Apache-2.0, đều được cấp phép kép như trên, không kèm điều khoản bổ sung nào.

PHP, MariaDB và Composer được tải từ nguồn chính thức khi chạy và giữ giấy phép riêng của chúng.
Dashboard kèm [Preact](https://preactjs.com/), [htm](https://github.com/developit/htm) (MIT) và
[uPlot](https://github.com/leeoniya/uPlot) (MIT); giấy phép của chúng nằm trong
`crates/daemon/dashboard/vendor/`.

## Lời cảm ơn

- [Laravel Herd](https://herd.laravel.com/) và [Laravel Valet](https://laravel.com/docs/valet), vì đã
  cho thấy lập trình PHP local có thể dễ chịu tới mức nào.
- [hyper](https://hyper.rs/), [tokio](https://tokio.rs/), [rustls](https://github.com/rustls/rustls)
  và [rcgen](https://github.com/rustls/rcgen), nền tảng của dịch vụ.
- [windows.php.net](https://windows.php.net/), [MariaDB Foundation](https://mariadb.org/) và
  [Composer](https://getcomposer.org/), vì bản build Windows chính thức và API tải về.

PHPev không liên kết với Laravel, PHP, MariaDB hay Microsoft.
