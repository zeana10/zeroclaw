/**
 * useVoice — browser voice input (SpeechRecognition) and output (TTS via
 * the ZeroClaw /companion/tts gateway endpoint with Web Speech API fallback).
 *
 * Usage:
 *   const { listening, speaking, supported, startListening, stopListening,
 *           speak, cancelSpeech } = useVoice({ onTranscript, gatewayTts });
 */

import { useCallback, useEffect, useRef, useState } from 'react';

// ── Browser type declarations ────────────────────────────────────

interface SpeechRecognitionEvent extends Event {
  results: SpeechRecognitionResultList;
}

interface SpeechRecognitionErrorEvent extends Event {
  error: string;
}

interface SpeechRecognition extends EventTarget {
  continuous: boolean;
  interimResults: boolean;
  lang: string;
  onresult: ((e: SpeechRecognitionEvent) => void) | null;
  onerror: ((e: SpeechRecognitionErrorEvent) => void) | null;
  onend: (() => void) | null;
  start(): void;
  stop(): void;
  abort(): void;
}

declare global {
  interface Window {
    SpeechRecognition?: new () => SpeechRecognition;
    webkitSpeechRecognition?: new () => SpeechRecognition;
  }
}

// ── Hook options / return ────────────────────────────────────────

export interface UseVoiceOptions {
  /** Called with the final transcript when recognition completes. */
  onTranscript: (text: string) => void;
  /** When true, POST text to /companion/tts for high-quality synthesis.
   *  Falls back to Web Speech API SpeechSynthesis if false or on error. */
  gatewayTts?: boolean;
  /** BCP-47 language tag for recognition, e.g. "en-US". Default: browser default. */
  lang?: string;
}

export interface UseVoiceReturn {
  /** True while the microphone is active. */
  listening: boolean;
  /** True while TTS audio is playing. */
  speaking: boolean;
  /** True if SpeechRecognition is available in this browser. */
  supported: boolean;
  startListening: () => void;
  stopListening: () => void;
  /** Speak `text` aloud using gateway TTS (if enabled) or Web Speech. */
  speak: (text: string) => Promise<void>;
  cancelSpeech: () => void;
}

// ── Hook implementation ──────────────────────────────────────────

export function useVoice({
  onTranscript,
  gatewayTts = false,
  lang,
}: UseVoiceOptions): UseVoiceReturn {
  const [listening, setListening] = useState(false);
  const [speaking, setSpeaking] = useState(false);

  const recognitionRef = useRef<SpeechRecognition | null>(null);
  const audioRef = useRef<HTMLAudioElement | null>(null);

  // Detect browser support
  const SpeechRecognitionCtor =
    typeof window !== 'undefined'
      ? window.SpeechRecognition ?? window.webkitSpeechRecognition
      : undefined;
  const supported = Boolean(SpeechRecognitionCtor);

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      recognitionRef.current?.abort();
      audioRef.current?.pause();
      if (typeof window !== 'undefined') {
        window.speechSynthesis?.cancel();
      }
    };
  }, []);

  const startListening = useCallback(() => {
    if (!SpeechRecognitionCtor || listening) return;

    const recognition = new SpeechRecognitionCtor();
    recognition.continuous = false;
    recognition.interimResults = false;
    if (lang) recognition.lang = lang;

    recognition.onresult = (e: SpeechRecognitionEvent) => {
      const transcript = Array.from(e.results)
        .map((r) => r[0]?.transcript ?? '')
        .join(' ')
        .trim();
      if (transcript) onTranscript(transcript);
    };

    recognition.onerror = (e: SpeechRecognitionErrorEvent) => {
      if (e.error !== 'no-speech') {
        console.warn('Speech recognition error:', e.error);
      }
      setListening(false);
    };

    recognition.onend = () => setListening(false);

    recognitionRef.current = recognition;
    recognition.start();
    setListening(true);
  }, [SpeechRecognitionCtor, listening, lang, onTranscript]);

  const stopListening = useCallback(() => {
    recognitionRef.current?.stop();
    setListening(false);
  }, []);

  const speakWithGateway = useCallback(async (text: string) => {
    const res = await fetch('/companion/tts', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ text }),
    });
    if (!res.ok) throw new Error(`TTS gateway error: ${res.status}`);
    const blob = await res.blob();
    const url = URL.createObjectURL(blob);
    const audio = new Audio(url);
    audioRef.current = audio;
    setSpeaking(true);
    return new Promise<void>((resolve, reject) => {
      audio.onended = () => {
        URL.revokeObjectURL(url);
        setSpeaking(false);
        resolve();
      };
      audio.onerror = (e) => {
        URL.revokeObjectURL(url);
        setSpeaking(false);
        reject(e);
      };
      audio.play().catch(reject);
    });
  }, []);

  const speakWithBrowser = useCallback((text: string): Promise<void> => {
    return new Promise((resolve) => {
      if (!window.speechSynthesis) {
        resolve();
        return;
      }
      window.speechSynthesis.cancel();
      const utt = new SpeechSynthesisUtterance(text);
      utt.onend = () => {
        setSpeaking(false);
        resolve();
      };
      utt.onerror = () => {
        setSpeaking(false);
        resolve();
      };
      setSpeaking(true);
      window.speechSynthesis.speak(utt);
    });
  }, []);

  const speak = useCallback(
    async (text: string) => {
      if (!text) return;
      if (gatewayTts) {
        try {
          await speakWithGateway(text);
          return;
        } catch (e) {
          console.warn('Gateway TTS failed, falling back to browser synthesis:', e);
        }
      }
      await speakWithBrowser(text);
    },
    [gatewayTts, speakWithGateway, speakWithBrowser],
  );

  const cancelSpeech = useCallback(() => {
    audioRef.current?.pause();
    audioRef.current = null;
    window.speechSynthesis?.cancel();
    setSpeaking(false);
  }, []);

  return { listening, speaking, supported, startListening, stopListening, speak, cancelSpeech };
}
