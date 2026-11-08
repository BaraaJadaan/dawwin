//! The messages Rust shows the user (errors, setup steps, dialog titles, export headings),
//! in the interface language. English is the key; other languages map from it.

use std::sync::atomic::{AtomicU8, Ordering};

pub const LANGUAGES: &[&str] = &["en", "ar"];

static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn set(lang: &str) {
    let i = LANGUAGES.iter().position(|l| *l == lang).unwrap_or(0);
    CURRENT.store(i as u8, Ordering::Relaxed);
}

pub fn lang() -> &'static str {
    LANGUAGES[CURRENT.load(Ordering::Relaxed) as usize]
}

/// `en` in the interface language (English when there is no translation). Each `{}` in it is
/// filled from `args`, in order.
pub fn tr(en: &str, args: &[String]) -> String {
    let template = match lang() {
        "ar" => AR.iter().find(|(e, _)| *e == en).map(|(_, a)| *a).unwrap_or(en),
        _ => en,
    };
    let mut out = String::with_capacity(template.len() + 16);
    let mut args = args.iter();
    let mut rest = template;
    while let Some(i) = rest.find("{}") {
        out.push_str(&rest[..i]);
        out.push_str(args.next().map(String::as_str).unwrap_or(""));
        rest = &rest[i + 2..];
    }
    out.push_str(rest);
    out
}

/// `tr!("Saved {}", name)`: [`tr`] with `Display` arguments.
#[macro_export]
macro_rules! tr {
    ($en:expr) => { $crate::i18n::tr($en, &[]) };
    ($en:expr, $($a:expr),+ $(,)?) => { $crate::i18n::tr($en, &[$(($a).to_string()),+]) };
}

static AR: &[(&str, &str)] = &[
    // queue
    ("The engine is not set up yet. Press “Set up engine” first.", "المحرّك غير مُعَدّ بعد. اضغط «إعداد المحرّك» أولًا."),
    ("Set up the engine first.", "أعِدّ المحرّك أولًا."),
    ("The download finished but the folder is not complete: {}", "انتهى التنزيل لكن المجلد غير مكتمل: {}"),
    ("That folder does not hold the model: {}.", "هذا المجلد لا يحوي النموذج: {}."),
    ("The model is not set up yet. Download or import it on the Engine and model page.", "النموذج غير مُعَدّ بعد. نزّله أو استورده من صفحة «المحرّك والنموذج»."),
    // setup
    ("That does not look like a Hugging Face token (they start with “hf_”).", "لا يبدو هذا رمز Hugging Face (تبدأ الرموز بـ «hf_»)."),
    ("Downloading uv {} (the tool that builds the Python environment)…", "جارٍ تنزيل uv {} (الأداة التي تبني بيئة Python)…"),
    ("The uv download is damaged (checksum mismatch). Press Set up again.", "تنزيل uv تالف (المجموع الاختباري لا يطابق). اضغط «إعداد المحرّك» مجددًا."),
    ("uv.exe was not in the downloaded archive", "لم يكن uv.exe في الأرشيف المنزَّل"),
    ("download failed: {}", "فشل التنزيل: {}"),
    ("NVIDIA driver {}: installing PyTorch for the GPU ({}).", "تعريف NVIDIA ‏{}: يُثبَّت PyTorch لمعالج الرسوميات ({})."),
    ("No NVIDIA GPU found: installing the CPU build of PyTorch. Transcription will be slower.", "لا يوجد معالج رسوميات NVIDIA: يُثبَّت إصدار PyTorch للمعالج. سيكون التفريغ أبطأ."),
    ("Creating a Python {} environment in {}", "جارٍ إنشاء بيئة Python {} في {}"),
    ("uv venv failed", "فشل uv venv"),
    ("Installing PyTorch and Transformers (about 1 GB the first time)…", "جارٍ تثبيت PyTorch وTransformers (نحو 1 غيغابايت في المرة الأولى)…"),
    ("Installing PyTorch with CUDA and Transformers (about 3 GB the first time)…", "جارٍ تثبيت PyTorch مع CUDA وTransformers (نحو 3 غيغابايت في المرة الأولى)…"),
    ("installing packages failed (press Set up again to continue)", "فشل تثبيت الحزم (اضغط «إعداد المحرّك» مجددًا للمتابعة)"),
    ("Checking the environment…", "جارٍ فحص البيئة…"),
    ("the new environment does not import torch", "البيئة الجديدة لا تستورد torch"),
    ("Engine ready.", "المحرّك جاهز."),
    ("{} is missing", "{} غير موجود"),
    ("config.json is not a Cohere Transcribe model", "config.json ليس لنموذج Cohere Transcribe"),
    ("model.safetensors is incomplete ({} GB)", "model.safetensors غير مكتمل ({} غيغابايت)"),
    // commands and dialogs
    ("Choose recordings to transcribe", "اختر التسجيلات المراد تفريغها"),
    ("Audio and video", "صوت وفيديو"),
    ("Choose a folder to transcribe", "اختر مجلدًا لتفريغه"),
    ("Transcribed, but the files could not be saved next to the recording: {}", "تم التفريغ، لكن تعذّر حفظ الملفات بجانب التسجيل: {}"),
    ("All files", "كل الملفات"),
    ("This transcript is still being written. Edit it when it is done.", "ما زال هذا التفريغ قيد الكتابة. عدّله بعد اكتماله."),
    ("A title cannot be empty.", "لا يمكن أن يكون العنوان فارغًا."),
    ("Export transcript", "تصدير التفريغ"),
    ("Text", "نص"),
    ("SubRip subtitles", "ترجمة SubRip"),
    ("WebVTT subtitles", "ترجمة WebVTT"),
    ("Word document", "مستند Word"),
    ("Where should the model (about 4 GB) be saved?", "أين يُحفظ النموذج (نحو 4 غيغابايت)؟"),
    ("Choose the folder with the model files (config.json, model.safetensors, tokenizer…)", "اختر مجلد ملفات النموذج (config.json وmodel.safetensors والمُرمِّز…)"),
    ("Dawwin · Arabic transcription", "دَوِّن · تحويل الكلام العربي إلى نص"),
    // audio
    ("ffmpeg could not read the file: {}", "تعذّر على ffmpeg قراءة الملف: {}"),
    ("unsupported format ({}); install ffmpeg for this file type", "صيغة غير مدعومة ({})؛ ثبّت ffmpeg لهذا النوع من الملفات"),
    ("unsupported codec ({}); install ffmpeg for this file type", "ترميز غير مدعوم ({})؛ ثبّت ffmpeg لهذا النوع من الملفات"),
    ("no audio track", "لا يوجد مسار صوتي"),
    ("no audio decoded", "لم يُقرأ أي صوت"),
    ("could not start ffmpeg", "تعذّر تشغيل ffmpeg"),
    ("cannot open {}", "تعذّر فتح {}"),
    // engine
    ("the engine stopped ({}). Last output:\n{}", "توقف المحرّك ({}). آخر ما كتبه:\n{}"),
    ("the engine exited ({}). Last output:\n{}", "خرج المحرّك ({}). آخر ما كتبه:\n{}"),
    ("the engine exited. Last output:\n{}", "خرج المحرّك. آخر ما كتبه:\n{}"),
    ("cannot start {}", "تعذّر تشغيل {}"),
    // updates
    ("Could not reach GitHub. Check the connection and try again.", "تعذّر الوصول إلى GitHub. تحقّق من الاتصال وحاول مجددًا."),
    ("No version has been published yet.", "لم يُنشر أي إصدار بعد."),
    ("GitHub answered {}. Try again later.", "ردّ GitHub بالرمز {}. حاول لاحقًا."),
    // exports
    ("Duration", "المدة"),
    ("Language", "اللغة"),
    ("Model", "النموذج"),
    ("Arabic", "العربية"),
    ("English", "الإنجليزية"),
    ("Duration {} · {}", "المدة {} · {}"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_and_fills_arguments() {
        set("en");
        assert_eq!(tr!("cannot open {}", "a.wav"), "cannot open a.wav");
        set("ar");
        assert_eq!(tr!("cannot open {}", "a.wav"), "تعذّر فتح a.wav");
        assert_eq!(tr!("no translation for this"), "no translation for this");
        set("en");
    }

    #[test]
    fn every_arabic_entry_keeps_its_placeholders() {
        for (en, ar) in AR {
            assert_eq!(en.matches("{}").count(), ar.matches("{}").count(), "{en}");
        }
    }
}
