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
    ("That folder does not hold a model file (.gguf).", "هذا المجلد لا يحوي ملف نموذج (.gguf)."),
    ("There is no model for this language yet. Choose one in Settings.", "لا يوجد نموذج لهذه اللغة بعد. اختر واحدًا من الإعدادات."),
    ("The speech engine is missing. Please install Dawwin again.", "محرّك الكلام غير موجود. أعد تثبيت دَوِّن من فضلك."),
    // models
    ("That file could not be read.", "تعذّرت قراءة ذلك الملف."),
    ("That is not a model file.", "هذا ليس ملف نموذج."),
    ("That model file is incomplete.", "ملف النموذج هذا غير مكتمل."),
    ("Dawwin cannot use that model. It works with Cohere Transcribe, Canary and Whisper model files.", "لا يستطيع دَوِّن استخدام ذلك النموذج. فهو يعمل مع ملفات نماذج Cohere Transcribe وCanary وWhisper."),
    ("The download arrived damaged, so it was removed. Please start it again.", "وصل التنزيل تالفًا فحُذف. ابدأه من جديد من فضلك."),
    ("The server refused the download (error {}).", "رفض الخادم التنزيل (الخطأ {})."),
    ("Not enough free space: the model needs {} GB more, and {} GB is free. Choose another folder or free some space.", "لا توجد مساحة كافية: يحتاج النموذج {} غيغابايت إضافية والمتاح {} غيغابايت. اختر مجلدًا آخر أو أفرغ بعض المساحة."),
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
    ("Where should the model be saved?", "أين يُحفظ النموذج؟"),
    ("Choose the model file (.gguf)", "اختر ملف النموذج (.gguf)"),
    ("Dawwin · Speech to text", "دَوِّن · تحويل الكلام إلى نص"),
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
    // translation
    ("There is no translation into that language.", "لا توجد ترجمة إلى تلك اللغة."),
    ("Dawwin cannot translate into that language.", "لا يستطيع دَوِّن الترجمة إلى تلك اللغة."),
    ("This transcript is still being written. Translate it when it is done.", "ما زال هذا التفريغ قيد الكتابة. ترجمه بعد اكتماله."),
    ("The transcript is already in that language.", "التفريغ بتلك اللغة أصلًا."),
    ("There is no translation model yet. Download one in Settings.", "لا يوجد نموذج ترجمة بعد. نزّل واحدًا من الإعدادات."),
    ("The translator is missing. Please install Dawwin again.", "المترجم غير موجود. أعد تثبيت دَوِّن من فضلك."),
    ("the translator exited ({}). Last output:\n{}", "خرج المترجم ({}). آخر ما كتبه:\n{}"),
    ("the translator exited. Last output:\n{}", "خرج المترجم. آخر ما كتبه:\n{}"),
    ("the translator did not start. Last output:\n{}", "لم يبدأ المترجم. آخر ما كتبه:\n{}"),
    ("Cancelled.", "أُلغي."),
    // subtitles into a video
    ("The video is not at {} any more.", "لم يعد الفيديو في {}."),
    ("That recording is not a video.", "هذا التسجيل ليس فيديو."),
    ("Adding subtitles to a video needs ffmpeg. Install it (winget install Gyan.FFmpeg), then try again.", "تحتاج إضافة النص إلى الفيديو إلى ffmpeg. ثبّته (winget install Gyan.FFmpeg) ثم أعد المحاولة."),
    ("Save the video with its subtitles", "حفظ الفيديو مع نصه"),
    ("Video", "فيديو"),
    ("Choose another name: the video itself cannot be written over.", "اختر اسمًا آخر: لا يمكن الكتابة فوق الفيديو نفسه."),
    ("ffmpeg could not add the subtitles: {}", "تعذّر على ffmpeg إضافة النص: {}"),
    // sound
    ("ffmpeg could not save the sound: {}", "تعذّر على ffmpeg حفظ الصوت: {}"),
    ("There is no speech in this recording.", "لا يوجد كلام في هذا التسجيل."),
    ("without silences", "دون فترات الصمت"),
    ("The recording is not at {} any more.", "لم يعد التسجيل في {}."),
    ("Choose another name: the recording itself cannot be written over.", "اختر اسمًا آخر: لا يمكن الكتابة فوق التسجيل نفسه."),
    ("Save the sound", "حفظ الصوت"),
    ("Sound", "صوت"),
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
