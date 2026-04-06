# mcopy - Asenkron Klasör Kopyalama Aracı

Windows sağ tık menüsü entegrasyonu ile hızlı ve performanslı dosya kopyalama aracı.

## Özellikler

- ✅ **Asenkron kopyalama**: Tokio ile paralel dosya işlemleri
- ✅ **Windows Context Menu**: Sağ tık ile "mcopy ile kopyala" ve "mcopy ile yapıştır"
- ✅ **Clipboard entegrasyonu**: Dosya yollarını clipboard üzerinden taşıma
- ✅ **Progress bar**: Terminal tabanlı ilerleme göstergesi
- ✅ **Optimal performans**: CPU core sayısına göre otomatik concurrency ayarı
- ✅ **Klasör optimizasyonu**: Hedef klasörleri önceden oluşturma

## Kurulum

### 1. Build

```bash
cargo build --release
```

### 2. Context Menu Kurulumu (Opsiyonel)

Admin PowerShell açın:

```powershell
# Sağ tık menüsünü kur
.\target\release\mcopy.exe install

# Kaldırmak için
.\target\release\mcopy.exe uninstall
```

## Kullanım

### CLI Mode (Legacy)

```bash
# Basit kullanım
mcopy kaynak_klasor hedef_klasor

# Concurrency ayarı
mcopy kaynak_klasor hedef_klasor -j 16

# Progress bar kapalı
mcopy kaynak_klasor hedef_klasor --no-progress
```

### Context Menu Mode

1. **Kopyala**: Dosya/klasöre sağ tık → "mcopy ile kopyala"
2. **Yapıştır**: Hedef klasörde boş alana sağ tık → "mcopy ile yapıştır"

### Manuel Clipboard Kullanımı

```bash
# Clipboard'a kopyala
mcopy copy C:\kaynak\dosya.txt

# Clipboard'tan yapıştır
mcopy paste C:\hedef\klasor
```

## Mimari

```
mcopy/
├── src/
│   ├── main.rs          # CLI orchestration ve subcommand routing
│   ├── lib.rs           # Shared kopyalama logic (async)
│   ├── clipboard.rs     # Clipboard yönetimi (arboard)
│   ├── context_menu.rs  # Registry işlemleri (winreg)
│   └── ui.rs            # UI modülü (gelecek sürümler için)
├── build.rs             # Windows manifest embedding
├── mcopy.rc             # Resource file
└── mcopy.manifest       # UAC manifest
```

## Registry Yapısı

```
HKLM\SOFTWARE\Classes\*\shell\mcopy_copy
    → "mcopy ile kopyala" (dosyalar için)

HKLM\SOFTWARE\Classes\Directory\shell\mcopy_copy
    → "mcopy ile kopyala" (klasörler için)

HKLM\SOFTWARE\Classes\Directory\Background\shell\mcopy_paste
    → "mcopy ile yapıştır" (boş alan için)
```

## Bağımlılıklar

- `tokio` - Async runtime
- `futures` - Stream processing
- `clap` - CLI parsing
- `indicatif` - Progress bars
- `winreg` - Windows Registry
- `arboard` - Clipboard
- `is_elevated` - Admin kontrolü
- `anyhow` - Error handling
- `num_cpus` - CPU core detection

## Performans

- **Concurrency**: Varsayılan `CPU cores × 4` (max 128)
- **Optimizasyon**: Klasörleri önceden oluşturma
- **Progress**: Her 100-200ms'de bir güncelleme

## Sınırlamalar

### MVP Sürümü
- ❌ GPUI UI henüz entegre değil (API karmaşıklığı nedeniyle)
- ✅ Terminal tabanlı progress bar kullanılıyor
- ✅ Temel context menu fonksiyonalitesi çalışıyor

### Gelecek Geliştirmeler (v2)
- Modern UI (GPUI veya Native Windows IProgressDialog)
- İkon desteği
- Çakışma çözümü (conflict resolution)
- Windows bildirimleri
- Çoklu seçim optimizasyonu

## Güvenlik

- Admin yetkisi sadece install/uninstall için gerekli
- Normal kullanımda admin yetkisi gerekmez
- Registry yazma: HKEY_LOCAL_MACHINE (tüm kullanıcılar için)

## Lisans

Bu proje eğitim amaçlıdır.

## Test

```bash
# Test klasörleri oluştur
mkdir test_src
echo "test" > test_src/file.txt

# Clipboard testi
.\target\release\mcopy.exe copy test_src
.\target\release\mcopy.exe paste test_dst

# Legacy mode testi
.\target\release\mcopy.exe test_src test_dst2

# Context menu testi (Admin PowerShell)
.\target\release\mcopy.exe install
# Dosya Gezgini'nde sağ tık menüsünü kontrol et
.\target\release\mcopy.exe uninstall
```

## Sorun Giderme

### "Admin yetkisi gerekli" Hatası
PowerShell'i "Run as Administrator" ile açın.

### "Clipboard'ta geçerli dosya/klasör yolu bulunamadı"
Önce "mcopy ile kopyala" komutunu kullanın.

### Registry Hatası
- Antivirüs yazılımı registry yazımını engelliyor olabilir
- Manuel olarak regedit ile kontrol edin

## Katkıda Bulunma

1. Fork edin
2. Feature branch oluşturun
3. Commit edin
4. Push edin
5. Pull Request açın

## Yazar

Bu proje Rust öğrenme sürecinde geliştirilmiştir.
