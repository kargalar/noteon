cargo build --release
<!-- ToDo -->


tasarımı en baştan yap

Genel arayüz stili
Sakin, mat ve kompakt bir koyu arayüz tasarla. Görsel kimliği antrasit yüzeyler, yumuşak köşeler ve kırık beyaz vurgular oluştursun. İçerik ekranda en çok dikkati çeksin. Kontroller düzenli gruplar halinde, kısa etiketlerle ve kolay taranabilir biçimde yerleşsin.
- Renk. Ana zeminde #101010, panel ve kartlarda #1C1C1C–`#252525`, giriş alanları ve etkileşim durumlarında #303030–`#414141` kullan. Ana metin #F4F4F4, yardımcı metin #929292 olsun. Birincil eylemlerde ve belirgin aktif kontrollerde #EEEEEE zemin üzerine #171717 metin kullan. Renkli vurguları içerik, etiket ve anlam taşıyan durumlar için ayır.
- Yüzeyler. Panelleri birbirinden yakın gri tonları ve gerektiğinde ince ayırıcı çizgilerle ayır. Buton ve giriş alanlarında 8–10 px, kartlarda 10–12 px, büyük panellerde ve diyaloglarda 16–18 px köşe yarıçapı kullan. Açılır menülere ve yüzen panellere yumuşak, geniş gölgeler ver.
- Kontroller. Birincil buton açık dolgulu, ikincil buton koyu gri, düşük öncelikli kontroller saydam zeminli olsun. İkonları sade ve tutarlı tut. Fareyle üzerine gelindiğinde yüzey bir ton açılsın. Seçimi açık bir dolgu veya ince çerçeveyle göster, klavye odağı için belirgin bir dış çizgi kullan.
- Yerleşim. İlişkili öğeleri birbirine yakın tut, gruplar arasında daha geniş boşluk bırak. Küçük kontroller arasında 6–10 px, panel içlerinde 16–24 px aralığı temel al. Sık kullanılan eylemleri görünür tut. Bağlamsal araçlar ve ayrıntılar ihtiyaç duyuldukları yerde açılabilsin.
- Tipografi. Segoe UI veya sade bir sistem yazı tipi kullan. Gövde metnini yaklaşık 14 px, kontrol etiketlerini 11–12 px, panel başlıklarını 16–20 px tut. Hiyerarşiyi yazı boyutu, ağırlık ve kontrastla kur. Kısa bölüm etiketlerinde hafifçe açılmış harf aralığı kullan.
- Hareket. Menü, panel ve durum geçişlerinde 150–300 ms süren hafif solma veya kısa kayma kullan. Hareket, neyin açıldığını ya da değiştiğini anlaşılır kılsın ve azaltılmış hareket tercihine uyum sağlasın.
<!-- Check -->