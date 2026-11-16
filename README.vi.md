<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/phpev-logo-dark.svg">
  <source media="(prefers-color-scheme: light)" srcset="assets/phpev-logo-light.svg">
  <img src="assets/phpev-logo-light.svg" alt="PHPev" width="420">
</picture>

**Môi trường PHP & Laravel local cho Windows: nhanh, nhẹ, mã nguồn mở.**

[English](README.md) · Tiếng Việt · [Demo dashboard](https://duy-lee.github.io/PHPev/)

</div>

PHPev cài PHP, MariaDB và Composer, cho mỗi thư mục project một địa chỉ như `http://shop.localhost`,
và cho mỗi project dùng version PHP riêng. Mọi thứ làm bằng lệnh `phpev` hoặc trên dashboard, dùng
hằng ngày không cần quyền admin.

## Tính năng

- **Site**: link một thư mục là có ngay `http://<tên>.localhost` (hoặc TLD khác như `.test`). Đổi
  tên site, chọn thư mục được phục vụ, bật HTTPS cho từng site. Rewrite rule trong `.htaccess` của
  Apache vẫn chạy.
- **Import project có sẵn**: `phpev import` chọn version PHP theo `composer.json`, bật extension
  còn thiếu, chạy `composer install`, tạo `.env` và database, rồi link thư mục.
- **PHP**: nhiều version song song (từ 7.0, bản build chính thức), hoặc thêm thư mục PHP có sẵn.
  Site dùng `.php-version`, version được ghim cho site, hoặc version mặc định. `php` và `composer`
  trong mọi terminal tự chạy đúng version của thư mục hiện tại. Bật tắt extension và sửa `php.ini`
  trên dashboard.
- **MariaDB**: nhiều version, mỗi version có database riêng; start, stop, restart, đổi version.
  Tạo, import, export, làm trống và xoá database; sửa `my.ini`. `mysql` và `mysqldump` trong mọi
  terminal kết nối sẵn tới nó.
- **Composer**: nhiều version song song, một version mặc định, site nào cần thì dùng version khác.
- **HTTPS**: certificate authority local, chỉ ký được tên miền local, chỉ được tin cho user của bạn.
- **Dashboard** (cửa sổ riêng, sáng hoặc tối, tiếng Anh hoặc tiếng Việt):
  - **Home**: cái gì đang chạy, cái gì cần xử lý, danh sách site.
  - **Sites**: mỗi site một trang gồm *Overview* (PHP, Composer, HTTPS, thư mục gốc, extension,
    kiểm tra kèm nút Fix), *Requests* (các request mới nhất), *Logs* và *Database*.
  - **Runtimes**: PHP, MariaDB (version, database, `my.ini`) và Composer.
  - **Monitor**: hiệu năng trực tiếp (request, độ trễ, route chậm nhất, CPU và RAM từng process)
    và log trực tiếp của PHPev, PHP, MariaDB và Laravel.
  - **Settings**: khởi động cùng Windows, icon khay, giao diện, ngôn ngữ, editor, TLD, port,
    HTTPS, Windows (Defender, file hosts, kiểm tra), dung lượng, giới thiệu.
  - `Ctrl K` để tìm site hoặc trang.
- **Icon khay**: mở dashboard hoặc site, start hoặc stop MariaDB, thoát.

## Yêu cầu

- Windows 10 (1809 trở lên) hoặc Windows 11, x64.
- Port 80 và 443 (site), 3306 (MariaDB), 7600 (dashboard) còn trống; đều đổi được.
- Có Internet để tải PHP, MariaDB và Composer.

## Cài đặt

Tải ở [Releases](https://github.com/duy-lee/PHPev/releases):

- **Installer** (`phpev-<version>-setup.exe`): chạy file, chọn thư mục chương trình và thư mục dữ
  liệu PHPev (mặc định `%USERPROFILE%\.phpev`). Sau đó PHPev có trong Start menu.
- **Zip** (`phpev-<version>-windows-x64.zip`): giải nén, rồi chạy `.\phpev.exe setup` trong thư
  mục đó. Lệnh này copy PHPev vào `%USERPROFILE%\.phpev` và thêm vào PATH.

Mở terminal mới và chạy `phpev doctor` để kiểm tra máy. File chưa được ký số: nếu Windows hiện
*"Windows protected your PC"*, chọn **More info → Run anyway**.

## Bắt đầu nhanh

```powershell
phpev php install 8.4      # version cài đầu tiên thành mặc định
cd C:\code\my-app
phpev import               # hoặc: phpev link
phpev dashboard
```

`http://my-app.localhost` giờ đã chạy project.

## Lệnh

| Lệnh | Mô tả |
|---|---|
| `phpev setup [--home DIR]` | Tạo thư mục PHPev, thêm vào PATH, kiểm tra máy |
| `phpev doctor [--fix-path] [--defender-exclude]` | Kiểm tra lỗi (PATH, port, chứng chỉ, …) |
| `phpev php list\|install\|add\|remove\|use\|ini\|which\|warm` | Version PHP |
| `phpev db list\|install\|remove\|start\|stop\|use` | Version MariaDB |
| `phpev composer install\|use\|self-update\|list\|remove\|pin\|unpin\|which` | Version Composer |
| `phpev link [NAME] [--php VER]` / `phpev unlink [NAME]` | Phục vụ / ngừng phục vụ thư mục hiện tại |
| `phpev rename <NEW>` | Đổi tên site |
| `phpev docroot [FOLDER] [--auto]` | Xem hoặc chọn thư mục site phục vụ |
| `phpev sites` | Liệt kê site |
| `phpev import [DIR] [--yes] [--migrate]` | Cho project có sẵn chạy được |
| `phpev secure [NAME]` / `phpev unsecure [NAME]` | HTTPS cho site |
| `phpev ca status\|uninstall` | Certificate authority local |
| `phpev tld [TLD]` | Xem hoặc đổi TLD (mặc định `localhost`) |
| `phpev hosts sync` | Ghi các site vào file hosts của Windows |
| `phpev start\|stop\|status` | Service chạy nền |
| `phpev autostart on\|off` | Khởi động PHPev khi đăng nhập |
| `phpev dashboard [--browser]` | Mở dashboard |
| `phpev tray on\|off` | Hiện hoặc ẩn icon khay |
| `phpev uninstall [--purge]` | Gỡ PHPev (hỏi có giữ database không) |

`phpev <lệnh> --help` để xem mọi tuỳ chọn.

## Cần biết

- **PHP nào được chạy:** file `.php-version` trong thư mục hoặc thư mục cha, rồi đến version được
  ghim cho site, rồi version mặc định. `phpev php which` cho biết lý do.
- **`.localhost` và PHP:** trình duyệt và curl tự hiểu `*.localhost`, nhưng các hàm PHP như
  `file_get_contents` thì không. Chạy `phpev hosts sync` một lần, hoặc dùng `phpev tld test`.
- **MariaDB** chạy ở `127.0.0.1:3306`, user `root`, không mật khẩu: chỉ dùng để phát triển local.
- **Cấu hình** nằm ở `%USERPROFILE%\.phpev\config.toml`; log ở `%USERPROFILE%\.phpev\logs`.
- **`php -v` ra PHP khác (XAMPP, Laragon):** PHP đó nằm trong PATH hệ thống, đứng trước PHPev.
  `phpev doctor --fix-path` đưa PHPev lên trước.
- **Port bị chiếm:** `phpev doctor` cho biết chương trình nào đang dùng; đổi port trong Settings.

## Build từ mã nguồn

Cần [Rust](https://rustup.rs/) (stable) và Visual Studio 2022 Build Tools (*Desktop development
with C++*).

```powershell
cargo build --release
.\target\release\phpev.exe setup
```

Không có Visual Studio thì dùng toolchain GNU với [WinLibs](https://winlibs.com/) trong PATH:
`cargo +stable-x86_64-pc-windows-gnu build --release`.

Demo dashboard (dữ liệu mẫu, không cần PHPev) chạy bằng `node demo/serve.mjs`.
