# mcopy

Windows sag tik menusu ile hizli dosya ve klasor kopyalama araci.

`mcopy`, secilen dosya veya klasor yollarini once clipboard'a alir, ardindan hedef klasorde tek tikla kopyalama baslatir. Kopyalama sirasinda GPUI tabanli bir pencere acilir ve ilerleme, duraklat/devam et ve durdur kontrolleri gorunur.

## Ozellikler

- Asenkron ve paralel kopyalama
- Windows Explorer context menu entegrasyonu
- Coklu secim destegi
- Clipboard tabanli copy/paste akisi
- GPUI ilerleme penceresi
- Duraklat, devam et ve durdur kontrolleri
- Legacy CLI modu

## Hizli Baslangic

### 1. Release build al

```powershell
cargo build --release
```

Olusan dosya:

```text
target\release\mcopy.exe
```

### 2. Context menu kur

Admin yetkili PowerShell ac ve proje klasorunde su komutu calistir:

```powershell
.\target\release\mcopy.exe install
```

Alternatif olarak dogrudan Cargo ile de calistirabilirsin:

```powershell
cargo run --release -- install
```

Bu komut su sag tik menu girdilerini ekler:

- Dosya uzerinde `mcopy ile kopyala`
- Klasor uzerinde `mcopy ile kopyala`
- Klasor uzerinde `mcopy ile buraya yapistir`
- Klasor boslugunda `mcopy ile yapistir`
- Disk kokunde `mcopy ile yapistir`

### 3. Context menu kaldir

Yine admin yetkili PowerShell ile:

```powershell
.\target\release\mcopy.exe uninstall
```

veya:

```powershell
cargo run --release -- uninstall
```

## Kurulum Notlari

- `install` ve `uninstall` komutlari Windows registry yazdigi icin admin yetkisi ister.
- Uygulamayi farkli bir klasore tasirsan veya yeni bir release exe olusturursan `install` komutunu tekrar calistirman iyi olur.
- Sag tik menusu eski exe'yi gosteriyorsa en guvenlisi once `uninstall`, sonra yeni exe ile tekrar `install` calistirmaktir.
- Sadece normal kullanimda admin gerekmez. Admin yetkisi yalnizca menu kurulum/kaldirma icindir.

## Kullanim

### Explorer uzerinden

1. Dosya veya klasoru sec.
2. Sag tik yapip `mcopy ile kopyala` sec.
3. Hedef klasore git.
4. Bos alanda veya klasor uzerinde sag tik yapip `mcopy ile yapistir` sec.
5. Acilan pencereden kopyalama durumunu izle.

Kopyalama penceresinde:

- `Duraklat` yeni islerin baslamasini bekletir
- `Devam Et` kuyrugu yeniden calistirir
- `Durdur` kalan kuyrugu iptal eder

Not:
Baslamis tekil `fs::copy` islemleri guvenli sekilde tamamlanir. Duraklatma ve durdurma kooperatif calisir; mevcut algoritma degistirilmeden yeni islerin baslamasi kontrol edilir.

### CLI modu

Legacy terminal kullanimini korumak icin su komutlar hala aktif:

```powershell
mcopy C:\kaynak C:\hedef
```

```powershell
mcopy C:\kaynak C:\hedef -j 16
```

```powershell
mcopy C:\kaynak C:\hedef --no-progress
```

### Manuel clipboard komutlari

```powershell
mcopy copy C:\kaynak\dosya.txt
```

```powershell
mcopy paste C:\hedef\klasor
```

```powershell
mcopy clear
```

## Proje Yapisi

```text
src/
|- main.rs
|- lib.rs
|- clipboard.rs
|- context_menu.rs
`- ui/
   |- mod.rs
   |- constants.rs
   |- progress.rs
   |- widgets.rs
   `- window.rs
```

Kisa aciklama:

- `main.rs`: CLI routing ve uygulama akisi
- `lib.rs`: ortak kopyalama mantigi ve kontrol mekanizmasi
- `clipboard.rs`: clipboard islemleri
- `context_menu.rs`: Windows registry context menu kurulumu
- `ui/`: GPUI pencere, bilesenler ve progress durumu

## Teknik Notlar

- Varsayilan concurrency: `CPU core sayisi x 4`
- Concurrency alt sinir: `4`
- Concurrency ust sinir: `128`
- Hedef klasorler kopyalama oncesi olusturulur
- Windows tarafinda UNC prefix temizligi uygulanir

## Sorun Giderme

### Sag tik menusu gorunmuyor

- Komutu admin PowerShell ile calistirdigindan emin ol.
- `.\target\release\mcopy.exe install` komutunu tekrar calistir.
- Gerekirse Explorer'i yeniden baslat veya oturumu kapatip ac.

### Eski UI aciliyor

- Muhtemelen registry eski exe yolunu kullaniyordur.
- `uninstall` calistir.
- Ardindan guncel release exe ile tekrar `install` calistir.

### "Admin yetkisi gerekli" hatasi

- PowerShell'i `Run as Administrator` ile ac.

### Clipboard'ta gecerli yol bulunamadi

- Once `mcopy ile kopyala` komutunu calistir.
- Kopyaladigin dosya veya klasorun hala mevcut oldugundan emin ol.

## Gelistirme

Yerel dogrulama icin:

```powershell
cargo fmt
```

```powershell
cargo check
```

```powershell
cargo clippy --all-targets -- -W unused -W dead_code -W unused_imports
```
