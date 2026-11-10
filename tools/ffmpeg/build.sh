#!/usr/bin/env bash
# A small ffmpeg.exe for Dawwin, cross-compiled for Windows on Linux (mingw-w64), LGPL only: no
# --enable-gpl, no --enable-nonfree, no GPL libraries.
#
#   tools/ffmpeg/build.sh audio   what Dawwin itself needs: read the sound of every recording and
#                                 video (Opus, AC-3, E-AC-3, DTS, ...), save it as M4A, and put
#                                 subtitle tracks into MP4 and MKV files
#   tools/ffmpeg/build.sh video   the same, and subtitles burned into the picture: video decoders,
#                                 libass with FreeType, HarfBuzz and FriBidi (Arabic shaping), and the
#                                 H.264 encoders of Windows (Media Foundation), NVIDIA and AMD
#
# Writes out/ffmpeg-<variant>.exe and out/ffmpeg-<variant>-configure.txt (what the build is made of,
# which the LGPL asks to be stated).
set -euo pipefail

VARIANT="${1:?audio or video}"
FFMPEG=8.1.2
FREETYPE=2.13.3
FRIBIDI=1.0.16
HARFBUZZ=10.1.0
LIBASS=0.17.3
HOST=x86_64-w64-mingw32
ROOT="$PWD"
WORK="$ROOT/build-$VARIANT"
PREFIX="$WORK/prefix"
OUT="$ROOT/out"
JOBS="$(nproc)"
mkdir -p "$WORK/src" "$PREFIX" "$OUT"
export PKG_CONFIG_LIBDIR="$PREFIX/lib/pkgconfig"
export PKG_CONFIG_PATH=""

fetch() { # url, directory it unpacks to
  local name; name="$(basename "$1")"
  [ -f "$WORK/src/$name" ] || curl -L --fail --retry 5 -o "$WORK/src/$name" "$1"
  [ -d "$WORK/src/$2" ] || tar -xf "$WORK/src/$name" -C "$WORK/src"
}

cat > "$WORK/cross.ini" <<EOF
[binaries]
c = '$HOST-gcc'
cpp = '$HOST-g++'
ar = '$HOST-ar'
strip = '$HOST-strip'
windres = '$HOST-windres'
pkg-config = 'pkg-config'
[host_machine]
system = 'windows'
cpu_family = 'x86_64'
cpu = 'x86_64'
endian = 'little'
EOF

meson_lib() { # source directory, then its options
  local dir="$1"; shift
  (cd "$WORK/src/$dir" && meson setup build --cross-file "$WORK/cross.ini" --prefix "$PREFIX" --libdir lib \
    --buildtype release -Ddefault_library=static "$@" && ninja -C build -j "$JOBS" && ninja -C build install)
}

EXTRA=()
if [ "$VARIANT" = video ]; then
  fetch "https://download.savannah.gnu.org/releases/freetype/freetype-$FREETYPE.tar.xz" "freetype-$FREETYPE"
  meson_lib "freetype-$FREETYPE" -Dbrotli=disabled -Dbzip2=disabled -Dharfbuzz=disabled -Dpng=disabled -Dzlib=disabled
  fetch "https://github.com/fribidi/fribidi/releases/download/v$FRIBIDI/fribidi-$FRIBIDI.tar.xz" "fribidi-$FRIBIDI"
  meson_lib "fribidi-$FRIBIDI" -Ddocs=false -Dbin=false -Dtests=false
  fetch "https://github.com/harfbuzz/harfbuzz/releases/download/$HARFBUZZ/harfbuzz-$HARFBUZZ.tar.xz" "harfbuzz-$HARFBUZZ"
  meson_lib "harfbuzz-$HARFBUZZ" -Dfreetype=enabled -Dglib=disabled -Dgobject=disabled -Dcairo=disabled -Dicu=disabled \
    -Dtests=disabled -Ddocs=disabled -Dutilities=disabled -Dintrospection=disabled
  fetch "https://github.com/libass/libass/releases/download/$LIBASS/libass-$LIBASS.tar.xz" "libass-$LIBASS"
  (cd "$WORK/src/libass-$LIBASS" && ./configure --host="$HOST" --prefix="$PREFIX" --enable-static --disable-shared \
    --disable-fontconfig --enable-directwrite --disable-require-system-font-provider && make -j "$JOBS" && make install)
  # NVIDIA's and AMD's encoder headers: headers only, MIT. NVIDIA's at 12.1, which drivers from 530 on run;
  # the newest ask for drivers newer than most computers have
  git clone --depth 1 --branch n12.1.14.0 https://github.com/FFmpeg/nv-codec-headers "$WORK/src/nv-codec-headers"
  make -C "$WORK/src/nv-codec-headers" install PREFIX="$PREFIX"
  git clone --depth 1 --filter=blob:none --sparse https://github.com/GPUOpen-LibrariesAndSDKs/AMF "$WORK/src/AMF"
  (cd "$WORK/src/AMF" && git sparse-checkout set amf/public/include)
  mkdir -p "$PREFIX/include/AMF" && cp -r "$WORK/src/AMF/amf/public/include/." "$PREFIX/include/AMF/"
  EXTRA=(
    --enable-swscale --enable-libass --enable-libfreetype --enable-libfribidi --enable-libharfbuzz
    --enable-mediafoundation --enable-d3d11va --enable-ffnvcodec --enable-nvenc --enable-amf
    --enable-decoder=h264,hevc,vp8,vp9,mpeg4,mpeg2video,mpeg1video,mjpeg,prores,h263,msmpeg4v3,wmv3,vc1
    --enable-encoder=h264_mf,hevc_mf,h264_nvenc,hevc_nvenc,h264_amf,hevc_amf
    --enable-filter=subtitles,ass,scale,format,null,setsar,pad,fps
    --extra-cflags="-I$PREFIX/include"
  )
else
  EXTRA=(--disable-swscale)
fi

fetch "https://ffmpeg.org/releases/ffmpeg-$FFMPEG.tar.xz" "ffmpeg-$FFMPEG"
cd "$WORK/src/ffmpeg-$FFMPEG"
./configure --prefix="$PREFIX" --target-os=mingw32 --arch=x86_64 --cross-prefix="$HOST-" \
  --pkg-config=pkg-config --pkg-config-flags=--static \
  --enable-static --disable-shared --extra-ldflags="-static -static-libgcc -static-libstdc++" --extra-libs="-lstdc++" \
  --disable-everything --disable-autodetect --disable-network --disable-doc --disable-debug \
  --disable-ffplay --disable-ffprobe --disable-avdevice --enable-small \
  --enable-protocol=file,pipe \
  --enable-demuxer=mov,matroska,ogg,mp3,aac,ac3,eac3,dts,dtshd,truehd,wav,w64,flac,amr,asf,avi,flv,mpegts,mpegps,aiff,caf,srt,webvtt,ass \
  --enable-decoder=aac,aac_latm,ac3,eac3,dca,truehd,mlp,mp1,mp2,mp3,mp3float,opus,vorbis,flac,alac,amrnb,amrwb,wmav1,wmav2,wmapro \
  --enable-decoder=pcm_s16le,pcm_s16be,pcm_s24le,pcm_s32le,pcm_f32le,pcm_f64le,pcm_u8,pcm_alaw,pcm_mulaw,adpcm_ima_wav,adpcm_ms,gsm_ms \
  --enable-decoder=srt,subrip,webvtt,movtext,ass,ssa \
  --enable-parser=aac,aac_latm,ac3,dca,mlp,mpegaudio,opus,vorbis,flac,h264,hevc,vp9,av1 \
  --enable-bsf=aac_adtstoasc,extract_extradata,h264_mp4toannexb,hevc_mp4toannexb \
  --enable-encoder=aac,pcm_s16le,pcm_f32le,movtext,srt,subrip,webvtt,ass \
  --enable-muxer=mp4,mov,ipod,matroska,webm,srt,webvtt,ass,pcm_f32le,wav,adts,null \
  --enable-filter=aresample,aformat,anull,atrim,asetpts,trim,setpts \
  "${EXTRA[@]}" | tee "$WORK/configure.log"
make -j "$JOBS"
"$HOST-strip" ffmpeg.exe
cp ffmpeg.exe "$OUT/ffmpeg-$VARIANT.exe"
{
  echo "FFmpeg $FFMPEG, built by tools/ffmpeg/build.sh $VARIANT. LGPL v2.1 or later; source: https://ffmpeg.org/releases/ffmpeg-$FFMPEG.tar.xz"
  [ "$VARIANT" = video ] && echo "With FreeType $FREETYPE, FriBidi $FRIBIDI, HarfBuzz $HARFBUZZ, libass $LIBASS, nv-codec-headers and AMF headers."
  grep -m1 FFMPEG_CONFIGURATION config.h
} > "$OUT/ffmpeg-$VARIANT-configure.txt"
ls -l "$OUT/ffmpeg-$VARIANT.exe"
