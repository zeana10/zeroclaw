import { useState, useEffect, useRef, useCallback } from 'react';
import { Send, AlertCircle, Copy, Check, Heart, Sparkles, Mic, MicOff, Volume2, VolumeX } from 'lucide-react';
import type { WsMessage } from '@/types/api';
import { WebSocketClient } from '@/lib/ws';
import { useVoice } from '@/hooks/useVoice';

interface ChatMessage {
  id: string;
  role: 'user' | 'companion';
  content: string;
  timestamp: Date;
}

// Emoji avatar keyed by companion name
function personaEmoji(name: string): string {
  const n = name.toLowerCase();
  if (n.startsWith('finn') || n.includes('fox')) return '🦊';
  if (n.startsWith('mara') || n.includes('bear')) return '🐻';
  if (n.startsWith('zeph') || n.includes('raven') || n.includes('crow')) return '🐦‍⬛';
  if (n.includes('wolf')) return '🐺';
  if (n.includes('cat') || n.includes('kit')) return '🐱';
  if (n.includes('rabbit') || n.includes('bunny')) return '🐇';
  if (n.includes('owl')) return '🦉';
  if (n.includes('deer')) return '🦌';
  return '🐾';
}

function personaAccent(name: string): { bubble: string; ring: string; dot: string; mic: string } {
  const n = name.toLowerCase();
  if (n.startsWith('finn') || n.includes('fox'))
    return { bubble: 'bg-orange-600', ring: 'ring-orange-500', dot: 'bg-orange-500', mic: 'bg-orange-600 hover:bg-orange-700' };
  if (n.startsWith('mara') || n.includes('bear'))
    return { bubble: 'bg-amber-700', ring: 'ring-amber-600', dot: 'bg-amber-500', mic: 'bg-amber-700 hover:bg-amber-800' };
  if (n.startsWith('zeph') || n.includes('raven') || n.includes('crow'))
    return { bubble: 'bg-violet-700', ring: 'ring-violet-500', dot: 'bg-violet-500', mic: 'bg-violet-700 hover:bg-violet-800' };
  return { bubble: 'bg-teal-700', ring: 'ring-teal-500', dot: 'bg-teal-500', mic: 'bg-teal-700 hover:bg-teal-800' };
}

interface PersonaInfo {
  name: string;
  description: string;
  tone: string;
}

const DEFAULT_PERSONA: PersonaInfo = {
  name: 'Companion',
  description: 'Your AI companion',
  tone: 'warm and friendly',
};

function familiarityLabel(turns: number): { label: string; pct: number } {
  if (turns === 0) return { label: 'Just met', pct: 0 };
  if (turns < 10) return { label: 'Getting acquainted', pct: Math.min(turns * 4, 40) };
  if (turns < 30) return { label: 'Warming up', pct: Math.min(40 + (turns - 10) * 2, 75) };
  if (turns < 60) return { label: 'Good friends', pct: Math.min(75 + (turns - 30), 95) };
  return { label: 'Old friends', pct: 100 };
}

export default function Companion() {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState('');
  const [typing, setTyping] = useState(false);
  const [connected, setConnected] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [persona] = useState<PersonaInfo>(DEFAULT_PERSONA);
  const [turnCount, setTurnCount] = useState(0);
  const [voiceMode, setVoiceMode] = useState(false);

  const wsRef = useRef<WebSocketClient | null>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const pendingContentRef = useRef('');
  const [copiedId, setCopiedId] = useState<string | null>(null);

  const emoji = personaEmoji(persona.name);
  const accent = personaAccent(persona.name);
  const { label: famLabel, pct: famPct } = familiarityLabel(turnCount);

  // ── Voice ──────────────────────────────────────────────────────
  const { listening, speaking, supported: voiceSupported, startListening, stopListening, speak, cancelSpeech } = useVoice({
    onTranscript: (text) => {
      setInput(text);
      // Auto-send in voice mode
      if (voiceMode) {
        sendMessage(text);
      }
    },
    gatewayTts: voiceMode,
  });

  // ── WebSocket ──────────────────────────────────────────────────
  useEffect(() => {
    const ws = new WebSocketClient();
    ws.onOpen = () => { setConnected(true); setError(null); };
    ws.onClose = () => setConnected(false);
    ws.onError = () => setError('Connection lost. Reconnecting…');

    ws.onMessage = (msg: WsMessage) => {
      switch (msg.type) {
        case 'chunk':
          setTyping(true);
          pendingContentRef.current += msg.content ?? '';
          break;

        case 'message':
        case 'done': {
          const content = msg.full_response ?? msg.content ?? pendingContentRef.current;
          if (content) {
            setMessages((prev) => [
              ...prev,
              { id: crypto.randomUUID(), role: 'companion', content, timestamp: new Date() },
            ]);
            setTurnCount((c) => c + 1);
            // Auto-speak companion response in voice mode
            if (voiceMode) {
              speak(content).catch(console.warn);
            }
          }
          pendingContentRef.current = '';
          setTyping(false);
          break;
        }

        case 'error':
          setMessages((prev) => [
            ...prev,
            {
              id: crypto.randomUUID(),
              role: 'companion',
              content: `Something went wrong: ${msg.message ?? 'unknown error'}`,
              timestamp: new Date(),
            },
          ]);
          setTyping(false);
          pendingContentRef.current = '';
          break;
      }
    };

    ws.connect();
    wsRef.current = ws;
    return () => ws.disconnect();
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages, typing]);

  const sendMessage = useCallback((text: string) => {
    const trimmed = text.trim();
    if (!trimmed || !wsRef.current?.connected) return;

    setMessages((prev) => [
      ...prev,
      { id: crypto.randomUUID(), role: 'user', content: trimmed, timestamp: new Date() },
    ]);

    try {
      wsRef.current.sendMessage(trimmed);
      setTyping(true);
      pendingContentRef.current = '';
    } catch {
      setError('Failed to send message. Please try again.');
    }

    setInput('');
    if (inputRef.current) {
      inputRef.current.style.height = 'auto';
      inputRef.current.focus();
    }
  }, []);

  const handleSend = () => sendMessage(input);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  const handleTextareaChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    setInput(e.target.value);
    e.target.style.height = 'auto';
    e.target.style.height = `${Math.min(e.target.scrollHeight, 200)}px`;
  };

  const handleCopy = useCallback((msgId: string, content: string) => {
    navigator.clipboard.writeText(content).then(() => {
      setCopiedId(msgId);
      setTimeout(() => setCopiedId((prev) => (prev === msgId ? null : prev)), 2000);
    });
  }, []);

  const toggleMic = () => {
    if (listening) stopListening();
    else startListening();
  };

  const toggleVoiceMode = () => {
    setVoiceMode((v) => !v);
    cancelSpeech();
  };

  return (
    <div className="flex flex-col h-[calc(100vh-3.5rem)]">
      {/* Persona header */}
      <div className="border-b border-gray-800 bg-gray-900/60 backdrop-blur px-6 py-3 flex items-center gap-4">
        <div className={`w-12 h-12 rounded-2xl flex items-center justify-center text-2xl ring-2 ${accent.ring} bg-gray-800 flex-shrink-0`}>
          {emoji}
        </div>
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2">
            <h2 className="text-white font-semibold text-lg leading-tight">{persona.name}</h2>
            <Sparkles className="h-4 w-4 text-yellow-400" />
          </div>
          <p className="text-gray-400 text-xs truncate">{persona.description}</p>
        </div>

        {/* Familiarity meter */}
        <div className="flex-shrink-0 text-right hidden sm:block">
          <div className="flex items-center gap-1.5 justify-end mb-1">
            <Heart className="h-3.5 w-3.5 text-pink-400" />
            <span className="text-xs text-gray-400">{famLabel}</span>
          </div>
          <div className="w-32 h-1.5 bg-gray-700 rounded-full overflow-hidden">
            <div
              className="h-full bg-gradient-to-r from-pink-500 to-rose-400 rounded-full transition-all duration-700"
              style={{ width: `${famPct}%` }}
            />
          </div>
        </div>

        {/* Voice mode toggle */}
        {voiceSupported && (
          <button
            onClick={toggleVoiceMode}
            title={voiceMode ? 'Disable voice mode' : 'Enable voice mode'}
            className={`flex-shrink-0 p-2 rounded-xl transition-colors ${
              voiceMode
                ? `${accent.bubble} text-white`
                : 'text-gray-500 hover:text-gray-300 hover:bg-gray-800'
            }`}
          >
            {voiceMode ? <Volume2 className="h-5 w-5" /> : <VolumeX className="h-5 w-5" />}
          </button>
        )}

        {/* Speaking indicator */}
        {speaking && (
          <div className="flex-shrink-0 flex items-center gap-1">
            {[0, 1, 2].map((i) => (
              <span
                key={i}
                className={`w-1 rounded-full ${accent.dot} animate-bounce`}
                style={{ height: `${8 + i * 4}px`, animationDelay: `${i * 100}ms` }}
              />
            ))}
          </div>
        )}

        {/* Connection dot */}
        <span className={`flex-shrink-0 h-2 w-2 rounded-full ${connected ? accent.dot : 'bg-red-500'} ${connected ? 'animate-pulse' : ''}`} />
      </div>

      {/* Error bar */}
      {error && (
        <div className="px-4 py-2 bg-red-900/30 border-b border-red-800 flex items-center gap-2 text-sm text-red-300">
          <AlertCircle className="h-4 w-4 flex-shrink-0" />
          {error}
        </div>
      )}

      {/* Voice mode banner */}
      {voiceMode && (
        <div className={`px-4 py-2 ${accent.bubble}/20 border-b border-gray-700 flex items-center justify-center gap-2 text-xs text-gray-300`}>
          <Mic className="h-3.5 w-3.5" />
          Voice mode on — tap the mic to speak, {persona.name} will talk back
        </div>
      )}

      {/* Messages */}
      <div className="flex-1 overflow-y-auto px-4 py-6 space-y-5">
        {messages.length === 0 && (
          <div className="flex flex-col items-center justify-center h-full gap-3 text-gray-500 select-none">
            <span className="text-6xl">{emoji}</span>
            <p className="text-lg font-medium text-gray-300">{persona.name} is ready to chat</p>
            <p className="text-sm text-gray-500">
              {persona.tone.charAt(0).toUpperCase() + persona.tone.slice(1)} · remembers everything
            </p>
            {voiceSupported && (
              <p className="text-xs text-gray-600 mt-1">
                Tap <Volume2 className="inline h-3.5 w-3.5" /> in the header to enable voice
              </p>
            )}
          </div>
        )}

        {messages.map((msg) => {
          const isUser = msg.role === 'user';
          return (
            <div key={msg.id} className={`group flex items-end gap-3 ${isUser ? 'flex-row-reverse' : ''}`}>
              <div className={`flex-shrink-0 w-8 h-8 rounded-xl flex items-center justify-center text-base ${isUser ? 'bg-gray-700' : 'bg-gray-800'}`}>
                {isUser ? '🧑' : emoji}
              </div>
              <div className="relative max-w-[72%]">
                <div className={`rounded-2xl px-4 py-3 ${isUser ? `${accent.bubble} text-white rounded-br-sm` : 'bg-gray-800 text-gray-100 border border-gray-700/60 rounded-bl-sm'}`}>
                  <p className="text-sm whitespace-pre-wrap break-words leading-relaxed">{msg.content}</p>
                  <p className={`text-[10px] mt-1.5 ${isUser ? 'text-white/50 text-right' : 'text-gray-500'}`}>
                    {msg.timestamp.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
                  </p>
                </div>
                <button
                  onClick={() => handleCopy(msg.id, msg.content)}
                  aria-label="Copy"
                  className="absolute top-1 right-1 opacity-0 group-hover:opacity-100 transition-opacity p-1 rounded bg-gray-700/80 hover:bg-gray-600 text-gray-400 hover:text-white"
                >
                  {copiedId === msg.id ? <Check className="h-3 w-3 text-green-400" /> : <Copy className="h-3 w-3" />}
                </button>
              </div>
            </div>
          );
        })}

        {typing && (
          <div className="flex items-end gap-3">
            <div className="flex-shrink-0 w-8 h-8 rounded-xl bg-gray-800 flex items-center justify-center text-base">{emoji}</div>
            <div className="bg-gray-800 border border-gray-700/60 rounded-2xl rounded-bl-sm px-4 py-3">
              <div className="flex items-center gap-1">
                {[0, 150, 300].map((delay) => (
                  <span key={delay} className="w-2 h-2 bg-gray-400 rounded-full animate-bounce" style={{ animationDelay: `${delay}ms` }} />
                ))}
              </div>
            </div>
          </div>
        )}

        <div ref={messagesEndRef} />
      </div>

      {/* Input area */}
      <div className="border-t border-gray-800 bg-gray-900 p-4">
        <div className="flex items-end gap-3 max-w-3xl mx-auto">

          {/* Mic button */}
          {voiceSupported && (
            <button
              onClick={toggleMic}
              title={listening ? 'Stop listening' : 'Start voice input'}
              className={`flex-shrink-0 p-3 rounded-2xl transition-all ${
                listening
                  ? 'bg-red-600 hover:bg-red-700 text-white animate-pulse ring-2 ring-red-400'
                  : `${accent.mic} text-white`
              }`}
            >
              {listening ? <MicOff className="h-5 w-5" /> : <Mic className="h-5 w-5" />}
            </button>
          )}

          <div className="flex-1">
            <textarea
              ref={inputRef}
              rows={1}
              value={input}
              onChange={handleTextareaChange}
              onKeyDown={handleKeyDown}
              placeholder={
                listening
                  ? 'Listening…'
                  : connected
                  ? `Message ${persona.name}…`
                  : 'Connecting…'
              }
              disabled={!connected || listening}
              className="w-full bg-gray-800 border border-gray-700 rounded-2xl px-4 py-3 text-sm text-white placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-offset-0 focus:border-transparent disabled:opacity-50 resize-none overflow-y-auto"
              style={{ minHeight: '44px', maxHeight: '200px' }}
            />
          </div>

          <button
            onClick={handleSend}
            disabled={!connected || !input.trim() || listening}
            className={`flex-shrink-0 ${accent.bubble} hover:opacity-90 disabled:bg-gray-700 disabled:text-gray-500 text-white rounded-2xl p-3 transition-all`}
          >
            <Send className="h-5 w-5" />
          </button>
        </div>

        {/* Status row */}
        <div className="flex items-center justify-center mt-2 gap-3 text-xs text-gray-500">
          <div className="flex items-center gap-1.5">
            <span className={`h-1.5 w-1.5 rounded-full ${connected ? accent.dot : 'bg-red-500'}`} />
            {connected ? 'Connected' : 'Disconnected'}
          </div>
          {voiceMode && (
            <>
              <span>·</span>
              <div className="flex items-center gap-1.5">
                <Mic className="h-3 w-3" />
                {listening ? 'Listening…' : speaking ? 'Speaking…' : 'Voice on'}
              </div>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
