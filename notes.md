✦ Researching Rust's audio ecosystem reveals several strong options for building an audio comparison
  library focused on fingerprinting.

  1. Core Fingerprinting Algorithms
  For your use case (identifying the same track across different qualities/formats), two main
  approaches stand out:

   * Chromaprint (AcoustID): This is the industry standard for identifying audio files. It is
     designed to be robust against different encodings, bitrates, and even slight speed changes.
       * Recommended Crate: chromaprint-next (https://crates.io/crates/chromaprint-next) is a
         high-performance, pure-Rust implementation. It generates a sequence of 32-bit integers (the
         fingerprint).
   * Shazam-style (Landmark-based): These algorithms focus on "peaks" in the spectrogram. They are
     extremely robust to noise (e.g., recording via a microphone), but can be more complex to
     implement for a database search compared to Chromaprint.
       * Recommended Crate: songrec (https://crates.io/crates/songrec) or shezem-rs
         (https://github.com/mre/shezem-rs) provide pure-Rust implementations of these techniques.

  2. Audio Decoding
  Before fingerprinting, you must decode the audio (MP3, FLAC, AAC, etc.) into raw PCM samples
  (usually 16-bit mono at a specific sample rate, like 11025Hz for Chromaprint).
   * Recommended Crate: symphonia (https://crates.io/crates/symphonia). It is the most robust
     pure-Rust decoding library, supporting almost all major formats.

  3. Database Comparison (Similarity Search)
  Storing and comparing fingerprints is where the "good enough" part comes in:
   * Bit-error rate / Hamming Distance: For Chromaprint, you compare the sequences of integers. Two
     tracks are considered the same if their fingerprints have a high percentage of matching bits.
   * Locality Sensitive Hashing (LSH): If your database grows large, you cannot do a linear search
     (comparing your new fingerprint against every existing one). LSH allows you to find
     "approximate" matches very quickly.
       * Recommended Crate: lsh-rs (https://crates.io/crates/lsh-rs) for efficient similarity
         searching.

  Suggested Direction
  Given your goal, chromaprint-next combined with symphonia is likely the best starting point.
  Chromaprint was specifically designed for the "same track, different file" problem.

  Would you like to dive deeper into how to integrate symphonia with chromaprint-next, or are you
  more interested in the database storage and search aspect (LSH)?
▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀

**Metadata Note:**
While `symphonia` is great for audio decoding and has metadata support, if we only need comprehensive and robust metadata parsing (tags) without decoding, `lofty` is a very strong and specialized alternative to keep in mind. I've left metadata reading for later to see if `symphonia` fully satisfies the requirements, but `lofty` remains a solid option.