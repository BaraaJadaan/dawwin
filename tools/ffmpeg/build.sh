#!/usr/bin/env bash
# A small ffmpeg.exe for Dawwin, cross-compiled for Windows on Linux (mingw-w64), LGPL only: no
# --enable-gpl, no --enable-nonfree, no GPL libraries.
#
#   tools/ffmpeg/build.sh audio   what Dawwin itself needs: read the sound of every recording and
#                                 video (Opus, AC-3, E-AC-3, DTS, ...), save it as M4A, and put
#                                 subtitle tracks into MP4 and MKV files
#   tools/ffmpeg/build.sh video   the same, and subtitles burned into the picture: video decoders
#                                 (AV1 through dav1d), libass with FreeType, HarfBuzz and FriBidi
#                                 (Arabic shaping), and the H.264 encoders of Windows (Media
#                                 Foundation), NVIDIA and AMD
#
# Both read raw 32-bit float sound (f32le) and take "-" as input (the fd protocol). The second
# builds (Dawwin 1.4.0) added those and dav1d; the app records which build it unpacked.
#
# Writes out/ffmpeg-<variant>/: ffmpeg.exe, README.txt (what the build is made of and where its
# source is, which the LGPL asks to be stated) and the licenses of what is in it. tools/pack.py packs
# that folder into the file Dawwin downloads.
set -euo pipefail

VARIANT="${1:?audio or video}"
FFMPEG=8.1.2
FREETYPE=2.13.3
FRIBIDI=1.0.16
HARFBUZZ=10.1.0
LIBASS=0.17.3
DAV1D=1.5.1
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
  fetch "https://downloads.videolan.org/pub/videolan/dav1d/$DAV1D/dav1d-$DAV1D.tar.xz" "dav1d-$DAV1D"
  meson_lib "dav1d-$DAV1D" -Denable_tools=false -Denable_tests=false -Denable_examples=false
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
    --enable-swscale --enable-libass --enable-libfreetype --enable-libfribidi --enable-libharfbuzz --enable-libdav1d
    --enable-mediafoundation --enable-d3d11va --enable-ffnvcodec --enable-nvenc --enable-amf
    --enable-decoder=h264,hevc,vp8,vp9,mpeg4,mpeg2video,mpeg1video,mjpeg,prores,h263,msmpeg4v3,wmv3,vc1,libdav1d
    --enable-encoder=h264_mf,hevc_mf,h264_nvenc,hevc_nvenc,h264_amf,hevc_amf
    --enable-filter=subtitles,ass,scale,format,null,setsar,pad,fps,transpose,hflip,vflip
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
  --enable-protocol=file,pipe,fd \
  --enable-demuxer=mov,matroska,ogg,mp3,aac,ac3,eac3,dts,dtshd,truehd,wav,w64,flac,amr,asf,avi,flv,mpegts,mpegps,aiff,caf,srt,webvtt,ass,pcm_f32le \
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
DEST="$OUT/ffmpeg-$VARIANT"
rm -rf "$DEST" && mkdir -p "$DEST"
cp ffmpeg.exe "$DEST/ffmpeg.exe"
cp COPYING.LGPLv2.1 "$DEST/LICENSE-ffmpeg.txt"
{
  echo "FFmpeg $FFMPEG for Dawwin (https://github.com/BaraaJadaan/dawwin), built by tools/ffmpeg/build.sh $VARIANT."
  echo "FFmpeg is free software under the GNU Lesser General Public License, version 2.1 or later (LICENSE-ffmpeg.txt)."
  echo "Its source: https://ffmpeg.org/releases/ffmpeg-$FFMPEG.tar.xz"
  if [ "$VARIANT" = video ]; then
    echo "With FreeType $FREETYPE, FriBidi $FRIBIDI, HarfBuzz $HARFBUZZ, libass $LIBASS and dav1d $DAV1D, whose licenses are next to this file,"
    echo "and the encoder headers of NVIDIA (nv-codec-headers n12.1.14.0) and AMD (AMF), both MIT."
  fi
  echo
  grep -m1 FFMPEG_CONFIGURATION config.h
} > "$DEST/README.txt"
if [ "$VARIANT" = video ]; then
  cp "$WORK/src/freetype-$FREETYPE/docs/FTL.TXT" "$DEST/LICENSE-freetype.txt"
  cp "$WORK/src/fribidi-$FRIBIDI/COPYING" "$DEST/LICENSE-fribidi.txt"
  cp "$WORK/src/harfbuzz-$HARFBUZZ/COPYING" "$DEST/LICENSE-harfbuzz.txt"
  cp "$WORK/src/libass-$LIBASS/COPYING" "$DEST/LICENSE-libass.txt"
  cp "$WORK/src/dav1d-$DAV1D/COPYING" "$DEST/LICENSE-dav1d.txt"
  cp "$WORK/src/AMF/LICENSE.txt" "$DEST/LICENSE-AMF.txt" 2>/dev/null || true
fi
ls -l "$DEST"
