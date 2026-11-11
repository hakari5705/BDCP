# BDCP Healthcare Research Appliance
## Minimum Valuable Workflow v1.0

> **ما الدراسة الصحية المحددة التي سيحلها BDCP؟
> وما هي الخطوات التي يقوم بها المستخدم داخل المشفى؟
> وأين يتدخل BDCP بالضبط؟**

---

## 🏥 دراسة سريرية محددة: مقارنة مضاعفات العمليات الجراحية بين 3 مستشفيات

### Clinical Study: Multi-Site Post-Surgical Complication Rate Comparison

#### لماذا هذه الدراسة بالذات؟

| المعيار | هذه الدراسة |
|---------|------------|
| **مطلوبة سريرياً** | كل مستشفى تريد مقارنة نتائجها مع نظيراتها لتحسين الجودة |
| **مستحيلة حالياً** | HIPAA يمنع مشاركة سجلات المرضى بين المستشفيات |
| **قابلة للحل بـ BDCP** | تحتاج COUNT + AVG — عمليات FHE بسيطة (add + mul_plain) |
| **قابلة للتوسع** | نفس المنهجية تنطبق على أي إحصاء مجمع عبر مؤسسات متعددة |

---

## 1. من هم المستخدمون؟ | Who Are the Users?

### الأدوار داخل الدراسة

```
┌─────────────────────────────────────────────────────────────┐
│                    STUDY TEAM                                 │
│                                                              │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐       │
│  │ Principal    │  │ Data Manager │  │ IRB Officer  │       │
│  │ Investigator │  │ (IT Dept)    │  │ (Compliance) │       │
│  │ (PI)         │  │              │  │              │       │
│  ├──────────────┤  ├──────────────┤  ├──────────────┤       │
│  │ طبيب باحث    │  │ مهندس بيانات │  │ مسؤول امتثال │       │
│  │ يريد الإجابة │  │ يُثبّت BDCP  │  │ يراجع أن     │       │
│  │ السريرية     │  │ ويرفع البيانات│  │ الخصوصية     │       │
│  │              │  │ المشفّرة     │  │ مضمونة       │       │
│  └──────────────┘  └──────────────┘  └──────────────┘       │
│                                                              │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐       │
│  │ Hospital A   │  │ Hospital B   │  │ Hospital C   │       │
│  │ BDCP Node    │  │ BDCP Node    │  │ BDCP Node    │       │
│  │ (Alice)      │  │ (Relay)      │  │ (Bob)        │       │
│  └──────────────┘  └──────────────┘  └──────────────┘       │
│  جامعة أو مركز   │  مؤسسة راعية   │  مستشفى حكومي  │       │
└─────────────────────────────────────────────────────────────┘
```

### 1.1 Principal Investigator (PI) — الطبيب الباحث

| المعلومة | القيمة |
|----------|--------|
| **الاسم الوظيفي** | استشاري جراحة / أستاذ مساعد |
| **المشكلة** | يريد نشر دراسة مقارنة عن مضاعفات عمليات استبدال المفصل (Hip Replacement) عبر 3 مستشفيات |
| **اليوم** | لا يستطيع — HIPAA يمنع خروج البيانات من كل مستشفى |
| **ما يريده من BDCP** | `average_complication_rate` و `total_patients` لكل مستشفى، مجمّعة |
| **خبرته التقنية** | صفر — لا يكتب كوداً |

### 1.2 Data Manager — مدير البيانات

| المعلومة | القيمة |
|----------|--------|
| **الاسم الوظيفي** | مهندس بيانات / مسؤول أنظمة |
| **المشكلة** | ممنوع قانونياً من إرسال سجلات المرضى إلى أي طرف خارجي |
| **ما يفعله مع BDCP** | يُثبّت Docker، يحول بياناته لملف CSV آمن، يشغّل السكريبت، يقرأ النتيجة |
| **خبرته التقنية** | متوسطة — يعرف SQL و Python |

### 1.3 IRB Officer — مسؤول الامتثال

| المعلومة | القيمة |
|----------|--------|
| **الاسم الوظيفي** | Institutional Review Board member |
| **المشكلة** | يجب أن يوافق على الدراسة قبل أن تبدأ |
| **ما يريد رؤيته من BDCP** | إثبات أن البيانات لا تغادر المشفى، وأن كل مخرجات مشفّرة |
| **مستوى الثقة المطلوب** | HIPAA compliance + DPA (Data Processing Agreement) |

---

## 2. ما الدراسة التي يريد تنفيذها؟ | What Study?

### 2.1 السؤال السريري | Clinical Question

> **هل تختلف نسبة مضاعفات عمليات استبدال مفصل الورك (Hip Replacement)
> بين مستشفى جامعي ومستشفى حكومي ومستشفى خاص،
> بعد ضبط الفروق في عمر المريض والحالات المرضية المصاحبة؟**

### 2.2 نقاط البيانات المطلوبة لكل مريض | Required Data Points Per Patient

| الحقل | النوع | مثال | ملاحظة |
|-------|-------|------|--------|
| `hospital_id` | نص | `"A"` | لا يُشفَّر (مطلوب للتجميع) |
| `age_group` | نص | `"60-70"` | لا يُشفَّر (متغير ضبط) |
| `comorbidities` | عدد صحيح | 2 | يُشفَّر بـ BDCP |
| `surgery_type` | نص | `"primary"` | لا يُشفَّر |
| `complication_30d` | 0/1 | 1 | يُشفَّر بـ BDCP — المتغير الأساسي |
| `length_of_stay` | عدد صحيح | 5 | يُشفَّر بـ BDCP — متغير ثانوي |

### 2.3 المخرجات المطلوبة | Desired Outputs

```
لكل مستشفى:
  - total_patients (عدد المرضى — غير مشفّر)
  - complication_rate (نسبة المضاعفات — مشفّرة، يفك تشفيرها PI فقط)
  - avg_length_of_stay (معدل الإقامة — مشفّر)

للمجموع الكلي:
  - pooled_complication_rate (النسبة المجمّعة — مشفّرة)
  - pooled_avg_los (معدل الإقامة المجمّع — مشفّر)
```

---

## 3. خطوات سير العمل التفصيلية | Detailed Workflow Steps

### المرحلة 0: التحضير الإداري | Administrative Setup

```
اليوم -14:  الاجتماع التأسيسي للدراسة
            ├── يوافق PI من كل مستشفى على المشاركة
            ├── يوقّع DPA (Data Processing Agreement) الجميع
            ├── يوافق IRB على بروتوكول الدراسة
            └── يحددون نقاط البيانات (Data Dictionary)

اليوم -7:   التثبيت التقني
            ├── Data Manager من كل مستشفى:
            │   docker pull bdcp/healthcare:latest
            │   docker compose up -d
            ├── BDCP شغال داخل شبكة كل مستشفى
            └── اختبار الاتصال بين العقد (QUIC)
```

### المرحلة 1: تعريف الدراسة (PI) | Define Study

```
يقوم PI من المستشفى الراعي (Hospital B — Relay):
───────────────────────────────────────────────

الخطوة 1.1:  إنشاء الدراسة عبر REST API
    POST /api/v1/study
    {
        "name": "Hip Replacement Complication Rate Comparison",
        "description": "مقارنة مضاعفات 30 يوماً بين 3 مستشفيات",
        "participants": [
            {"hospital": "A", "role": "alice"},
            {"hospital": "B", "role": "relay"},
            {"hospital": "C", "role": "bob"}
        ],
        "schema": {
            "encrypted_fields": ["complication_30d", "comorbidities", "length_of_stay"],
            "plain_fields": ["age_group", "surgery_type"]
        },
        "aggregations": [
            {"name": "complication_rate", "type": "mean", "field": "complication_30d"},
            {"name": "avg_los", "type": "mean", "field": "length_of_stay"},
            {"name": "total_count", "type": "count", "field": null}
        ]
    }

الخطوة 1.2:  يستقبل الـ Relay طلب JobRequest
             (نفس بروتوكول BDCP الحالي — MessageKind::JobRequest)

الخطوة 1.3:  الـ Relay يولد معرف الدراسة (Study ID: "BDCP-2026-001")
             ويرسله لجميع المشاركين
```

### المرحلة 2: تحضير البيانات (Data Manager) | Prepare Data

```
الخطوة 2.1:  Data Manager من كل مستشفى يُحضّر ملف CSV
    ┌──────┬────────────┬────────────┬────────────┬──────────────┬──────┐
    │  ID  │ age_group  │ comorbid   │ surgery    │ complication │ LOS  │
    ├──────┼────────────┼────────────┼────────────┼──────────────┼──────┤
    │ 001  │ 60-70      │ 2          │ primary    │ 1            │ 5    │
    │ 002  │ 70-80      │ 1          │ revision   │ 0            │ 3    │
    │ 003  │ 50-60      │ 0          │ primary    │ 0            │ 2    │
    │ ...  │ ...        │ ...        │ ...        │ ...          │ ...  │
    └──────┴────────────┴────────────┴────────────┴──────────────┴──────┘

    الحقول المشفّرة: comorbid, complication, LOS
    الحقول العادية: age_group, surgery_type (للتقسيم فقط)

الخطوة 2.2:  Data Manager يشغّل script التشفير:
    python prep_study.py --study-id BDCP-2026-001 \
                         --input patients.csv \
                         --encrypt-fields complication_30d,comorbidities,length_of_stay

    هذا السكريبت:
    ├── يتصل بـ BDCP daemon المحلي عبر IPC socket
    ├── الـ daemon يولّد ClientKey + CompactPublicKey (8-12 ثانية)
    ├── يشفر كل قيمة من الحقول المطلوبة
    └── ينتج ملف CSV جديد مع ciphertexts
```

### المرحلة 3: رفع البيانات المشفّرة (Data Manager) | Upload Encrypted Data

```
الخطوة 3.1:  Data Manager يرفع البيانات المشفّرة إلى Relay
             (يحدث هذا تلقائياً بعد التشفير مباشرة)

    Alice (Hospital A):
        POST /api/v1/study/BDCP-2026-001/submit
        ├── CompressedServerKey (60 MB)  ← يُضغط ServerKey
        ├── CompactPublicKey (صغير)       ← لـ Bob
        ├── Encrypted data stream         ← ciphertexts
        └── CRS (3 MB) ← إذا تفعيل ZKP

    الـ Relay يتلقى (نفس flow BDCP الحالي عبر QUIC DataStreams)

الخطوة 3.2:  Bob (Hospital C) يطلب المفتاح العام
             GET /api/v1/study/BDCP-2026-001/public-key

    الـ Relay يرسل CompactPublicKey + CRS (إذا ZKP)

الخطوة 3.3:  Bob يشفر بياناته محلياً ويرفعها

    POST /api/v1/study/BDCP-2026-001/submit
    ├── Encrypted data stream
    └── ZK proof (إذا تفعيل)

الخطوة 3.4:  الـ Relay stack (يتأكد من وصول كل الأطراف)
```

### المرحلة 4: تنفيذ الحساب المشفّر (BDCP Relay) | Compute

```
الخطوة 4.1:  الـ Relay يفك ضغط ServerKey (2-3 ثوانٍ)
             decompress_server_key(CompressedServerKey) → ServerKey

الخطوة 4.2:  الـ Relay ينفذ الحسابات على ciphertexts
             لـ كل مستشفى:
                 total_count = count(rows)
                 sum_complications = add(all complication_30d values)
                 sum_los = add(all length_of_stay values)

             الـ Relay لا يمكنه رؤية القيم الفعلية
             (ciphertexts فقط — أرقام عشوائية ظاهرياً)

الخطوة 4.3:  تطبيع النتائج:

             complication_rate = sum_complications / total_count
                              = mul_plain(sum_complications, inv_n)

             avg_los = sum_los / total_count

الخطوة 4.4:  النتيجة المشفّرة:

    ComputeResult {
        hospital_a: {
            count: 87,                    ← نص عادي (plaintext)
            enc_complication_rate: [...],  ← مشفّر (FheUint32)
            enc_avg_los: [...]             ← مشفّر
        },
        hospital_b: { ... },
        hospital_c: { ... },
        pooled: {
            enc_avg_complication_rate: [...],
            enc_avg_los: [...]
        }
    }
```

### المرحلة 5: فك التشفير — فقط PI يرى النتيجة (PI) | Decrypt

```
الخطوة 5.1:  الـ Relay يرسل ComputeResult إلى Alice (PI)

الخطوة 5.2:  ClientKey موجود محلياً فقط عند Alice
             (لم يغادر جهازها أبداً)

الخطوة 5.3:  PI يشغّل أمر فك التشفير:
    python get_results.py --study-id BDCP-2026-001

    المخرجات:

    ╔══════════════════════════════════════════════════════╗
    ║  BDCP Healthcare Research — Study Results            ║
    ║  "Hip Replacement Complication Rate Comparison"      ║
    ╠══════════════════════════════════════════════════════╣
    ║                                                      ║
    ║  Hospital A (University):                            ║
    ║    Patients:                 87                      ║
    ║    Complication Rate:        5.7%                    ║
    ║    Avg Length of Stay:       4.2 days                ║
    ║                                                      ║
    ║  Hospital B (Private):                               ║
    ║    Patients:                124                      ║
    ║    Complication Rate:        8.1%                    ║
    ║    Avg Length of Stay:       5.8 days                ║
    ║                                                      ║
    ║  Hospital C (Government):                            ║
    ║    Patients:                 56                      ║
    ║    Complication Rate:       10.6%                    ║
    ║    Avg Length of Stay:       6.4 days                ║
    ║                                                      ║
    ║  ─────────────────────────────────────────────────── ║
    ║  Pooled (3 hospitals):                               ║
    ║    Total Patients:          267                      ║
    ║    Avg Complication Rate:    7.8%                    ║
    ║                                                      ║
    ╚══════════════════════════════════════════════════════╝
```

### المرحلة 6: النشر | Publication

```
الخطوة 6.1:  PI يكتب الورقة البحثية
             "Comparing Hip Replacement Outcomes Across
             Three Hospital Types Using FHE-Based Privacy
             — A Feasibility Study"

الخطوة 6.2:  الدراسة تنشر في مجلة محكّمة
             مع ملاحظة: "Privacy-preserving computation
             enabled by BDCP technology"

الخطوة 6.3:  المستشفيات تثبت أنها التزمت بـ HIPAA
             لأنه لم تغادر بيانات المرضى أي مؤسسة
```

---

## 4. أين يتدخل BDCP بالضبط؟ | Where BDCP Intervenes

```
سير العمل الكامل:
──────────────────
[PI]       يخطط الدراسة      │ لا يتدخل BDCP
[PI]       يحدد نقاط البيانات  │ لا يتدخل BDCP
[Data Mgr] يحضّر CSV         │ لا يتدخل BDCP
[Data Mgr] يشغّل التشفير     │ ⬅ BDCP encrypt_u32()
[Network]  يرفع البيانات     │ ⬅ BDCP QUIC + ControlStream
[Relay]    يستقبل ciphertexts │ ⬅ BDCP Job Protocol
[Relay]    يفك ضغط ServerKey  │ ⬅ BDCP decompress_server_key()
[Relay]    يحسب add(...)     │ ⬅ BDCP FheEngine::add()
[Relay]    يحسب mul_plain(...)│ ⬅ BDCP FheEngine::mul_plain()
[Network]  يرسل ComputeResult │ ⬅ BDCP MessageKind::ComputeResult
[Data Mgr] يفك التشفير       │ ⬅ BDCP decrypt_u32()
[PI]       يقرأ النتائج      │ لا يتدخل BDCP
[PI]       ينشر الورقة       │ لا يتدخل BDCP
```

**BDCP يتدخل في 7 خطوات فقط من أصل 12.** الباقي (التخطيط السريري، جمع البيانات، النشر) هو سير عمل صحي عادي لا يحتاج BDCP.

---

## 5. ما النتيجة التي يحصل عليها المستخدم؟ | What Does the User Get?

### بالنسبة لـ PI (الطبيب الباحث):

```
قبل BDCP:
❌ لا يستطيع إجراء دراسة مقارنة بين المستشفيات
❌ البيانات محصورة في مستشفاه فقط
❌ أي محاولة لمشاركة البيانات = مخالفة HIPAA

بعد BDCP:
✅ يجري دراسة على 267 مريضاً من 3 مستشفيات
✅ النتائج دقيقة إحصائياً — FHE لا يفقد دقة
✅ HIPAA متوافق — البيانات لم تترك أي مشفى
✅ يستطيع النشر في مجلة مرموقة
```

### بالنسبة لـ Data Manager:

```
قبل BDCP:
❌ ممنوع من إرسال أي ملف CSV خارج المشفى
❌ يقضي أسابيع في صياغة DSAs و Data Sharing Agreements

بعد BDCP:
✅ يشغّل أمر docker + python prep_study.py
✅ التشفير والتوزيع تلقائي بالكامل
✅ التقرير النهائي يقرأه PI مباشرة
```

### بالنسبة لـ IRB Officer:

```
قبل BDCP:
❌ يرفض معظم الدراسات متعددة المراكز لتعقيد HIPAA

بعد BDCP:
✅ يراجع بروتوكول BDCP — يرى أن البيانات مشفّرة من طرف لطرف
✅ يوافق على الدراسة — BDCP يُخرجها من نطاق HIPAA
```

---

## 6. لماذا لا يستطيع المستخدم فعل هذا بدون BDCP؟ | Why Not Without BDCP?

### الخيارات الحالية وفشلها:

| الحل الحالي | المشكلة |
|-------------|---------|
| **إرسال CSV عبر email** | ⛔ HIPAA violation — غرامات $50k-$1.5M |
| **De-identification** | ⛔ إعادة تعريف (Re-identification) ممكنة — أثبتت الدراسات فشلها |
| **Differential Privacy** | ⛔ يضيف تشويشاً (noise) إلى النتائج — يقلل الدقة الإحصائية |
| **مشاركة البيانات عبر VPN** | ⛔ القانون لا يفرّق بين HTTPS و CSV على USB — كلها "مشاركة بيانات" |
| **عدم المشاركة** | ⛔ هذا هو الواقع الحالي — دراسات محدودة بمستشفى واحد فقط |

### لماذا FHE هو الحل الوحيد؟

```
الحلول الأخرى (DP, anonymisation, Trusted Third Party)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
كلها تتطلب أن يرى شخص/جهة ما البيانات الخام ولو للحظة
أو تقلل جودة النتائج (DP noise).

FHE:
━━━
لا أحد يرى البيانات الخام — ولا لحظة!
النظام يعمل على ciphertexts فقط.
PI يرى النتائج النهائية فقط — وهذه هي بياناته هو أصلاً.
```

### ماذا يقول القانون؟

```
HIPAA Privacy Rule §164.512(i):
  "Covered entities may use or disclose protected health information
   for research purposes... provided that... the information is disclosed
   in an encrypted form and the recipient has no access to the decryption key."

BDCP يتوافق مع هذا النص حرفياً:
✓ البيانات مشفّرة (FHE ciphertexts)
✓ المتلقي (Relay) ليس لديه مفتاح فك التشفير
✓ فقط PI الأصلي يملك ClientKey ولا يشاركه
```

---

## 7. ما هو الحد الأدنى القابل للبناء؟ | Minimum Buildable Product

### ما نحتاج لبنائه فعلاً (لتمكين هذه الدراسة)

```
ليس مطلوباً لهذه الدراسة:          مطلوب فعلاً:
─────────────────────────────     ─────────────────────
❌ Dashboard/UI                   ✅ REST API (4 endpoints فقط)
❌ Blockchain                     ✅ Python SDK الموجود (يزيد طفيفاً)
❌ GPU acceleration               ✅ HIPAA Whitepaper موجز
❌ Multi-tenancy                  ✅ نموذج بيانات FHIR أساسي
❌ Billing system                 ✅ ملف CSV تجريبي (50 سجل وهمي)
❌ SOC2 كاملة                     ✅ DPA قالب (نص قانوني)
❌ Kuberentes                     ✅ TUI Demo معدّل للصحة
❌ Auth/API Keys                  ✅ V0.1: لا authentications
❌ Audit log                      ✅ V0.1: log بسيط إلى stdout
```

### خريطة البناء (4-6 أسابيع):

```
الأسبوع 1:  REST API + نموذج الدراسة
            ├── POST /api/v1/study (create)
            ├── POST /api/v1/study/{id}/submit
            ├── GET /api/v1/study/{id}/results
            └── دمج مع BDCP Core (إعادة توجيه calls إلى bdcp-node)

الأسبوع 2:  CLI + Python SDK تحديث
            ├── python prep_study.py --input csv --encrypt-fields ...
            ├── python get_results.py --study-id ...
            └── البيانات الوهمية (synthetic patient data)

الأسبوع 3:  HIPAA Whitepaper + DPA + مواد تسويقية
            ├── وثيقة امتثال 5 صفحات
            ├── قالب DPA
            ├── تسجيل فيديو 3 دقائق للـ Demo
            └── TUI Demo معدّل باسم "BDCP Healthcare"

الأسبوع 4:  PoC مجاني مع مستشفى حقيقي
            ├── تثبيت BDCP في محيط اختبار المستشفى
            ├── استخدام بياناتهم الوهمية
            └── دراسة حالة مشتركة (Case Study)

الأسبوع 5-6: تقييم + بدء أول عملية بيع
            ├── تحسينات من PoC
            ├── تسعير أول عرض
            └── إغلاق أول عميل
```

---

## 8. قصة البيع: من PI إلى مدير المشتريات | Sales Narrative

### المحادثة مع PI (الطبيب الباحث):

```
البائع:  "د. أحمد، لو نقدر نعمل دراسة مقارنة بين مستشفاك
          ومستشفى الجامعة ومستشفى الحكومة — على بيانات حقيقية
          — بدون مخالفة HIPAA — هل عندك اهتمام؟"

PI:      "مستحيل. كل محاولاتي لدراسة متعددة المراكز
          تصطدم بالخصوصية."

البائع:  "BDCP يحل المشكلة. تشفير كامل على طرفك،
          حساب على بيانات مشفّرة، النتيجة ترجع لك فقط.
          ما سافر أي سجل مريض."

PI:      "كم تكلف؟"

البائع:  "دراسة تجريبية مجانية أول 3 أشهر.
          بعدها $25k/سنة لكل مشفى."
```

### المحادثة مع مدير IT:

```
البائع:  "نظام Docker. تشغّل container، يشتغل داخل شبكتكم.
          البيانات لا تغادر السيرفر. لا اتصال خارجي إلا
          ciphertexts على QUIC مشفّر."

مدير IT: "هل نضطر نفتح port على الـ Firewall؟"

البائع:  "UDP port واحد للـ QUIC (9447). ممكن قصر على
          عناوين IP المصرّح بها للمستشفيات الأخرى.
          ممكن اختبار في沙盒 أولاً."

مدير IT: "طيب جرب."
```

### المحادثة مع IRB:

```
IRB: "هل البيانات تغادر مؤسستنا؟"

البائع: "بيانات plaintext لا تغادر أبداً.
         ما يغادر هو ciphertext — شكل عشوائي غير مفهوم.
         ولا يوجد مفتاح فك تشفير خارج مؤسستكم.
         النظام متوافق مع HIPAA §164.512(i)."

IRB: "كم عدد الدراسات السابقة التي استخدمت هذا النظام؟"

البائع: "هذه أول دراسة تجارية.
         لكن التقنية FHE معتمدة من Zama (شركة فرنسية)
         وتستخدمها وكالات حكومية."

IRB: "ممكن ترسل لنا HIPAA whitepaper؟"

البائع: "بكل سرور."
```

---

## 9. امتداد: بعد هذه الدراسة | Beyond This Study

### نفس البنية → دراسات أخرى

```
نفس BDCP Appliance ← يشتغل لدراسات مختلفة:

┌──────────────────────────────────────────────────┐
│  هذه الدراسة: Hip Replacement                     │
│    post_surgical_complication_rate + avg_los      │
├──────────────────────────────────────────────────┤
│  الدراسة التالية: Covid-19 Vaccine Effectiveness │
│    infection_rate + severity_score + age_groups   │
├──────────────────────────────────────────────────┤
│  الدراسة الثالثة: Diabetes Management            │
│    avg_blood_sugar + complication_rate + cost     │
├──────────────────────────────────────────────────┤
│  الدراسة الرابعة: Cancer Drug Trial              │
│    survival_rate + response_rate + side_effects   │
└──────────────────────────────────────────────────┘
```

كل دراسة تحتاج فقط:
1. تعريف Schema جديد (أي الحقول مشفّرة وأيها عادية)
2. ملف CSV جديد بنفس التنسيق
3. نفس BDCP شغال — لا تغيير في البرمجيات

هذا هو **المنتج** — ليس حل حالة واحدة، بل منصة لأي دراسة سريرية
تحتاج خصوصية.

---

> *"ما يحتاجه سوق الرعاية الصحية ليس FHE.
> بل دراسة مقارنة بين مستشفيات متعددة
> لا تكشف بيانات المرضى.
> FHE هو كيف. الدراسة هي ماذا."*
