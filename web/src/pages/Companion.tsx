import { useState, useEffect, useRef, useCallback } from 'react';
import { Send, AlertCircle, Copy, Check, Heart, Sparkles } from 'lucide-react';
import type { WsMessage } from '@/types/api';
import { WebSocketClient } from '@/lib/ws';

interface ChatMessage {
  id: string;
  role: 'user' | 'companion';
  content: string;
  timestamp: Date;
}

// Emoji avatar keyed by companion name (case-insensitive prefix match)
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

// Accent colour per companion name
function personaAccent(name: string): { bubble: string; ring: string; dot: string } {
  const n = name.toLowerCase();
  if (n.startsWith('finn') || n.includes('fox'))
    return { bubble: 'bg-orange-600', ring: 'ring-orange-500', dot: 'bg-orange-500' };
  if (n.startsWith('mara') || n.includes('bear'))
    return { bubble: 'bg-amber-700', ring: 'ring-amber-600', dot: 'bg-amber-500' };
  if (n.startsWith('zeph') || n.includes('raven') || n.includes('crow'))
    return { bubble: 'bg-violet-700', ring: 'ring-violet-500', dot: 'bg-violet-500' };
  return { bubble: 'bg-teal-700', ring: 'ring-teal-500', dot: 'bg-teal-500' };
}

interface PersonaInfo {
  name: string;
  description: string;
  tone: string;
}

// Parse persona info out of the first companion message if it embeds one,
// otherwise fall back to a default based on config read from the gateway.
const DEFAULT_PERSONA: PersonaInfo = {
  name: 'Companion',
  description: 'Your AI companion',
  tone: 'warm and friendly',
};

// Familiarity label from turn count
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

  const wsRef = useRef<WebSocketClient | null>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const pendingContentRef = useRef('');
  const [copiedId, setCopiedId] = useState<string | null>(null);

  const emoji = personaEmoji(persona.name);
  const accent = personaAccent(persona.name);
  const { label: famLabel, pct: famPct } = familiarityLabel(turnCount);

  useEffect(() => {
    const ws = new WebSocketClient();

    ws.onOpen = () => {
      setConnected(true);
      setError(null);
    };
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
  }, []);

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages, typing]);

  const handleSend = () => {
    const trimmed = input.trim();
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
  };

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

  return (
    <div className="flex flex-col h-[calc(100vh-3.5rem)]">
      {/* Persona header */}
      <div className="border-b border-gray-800 bg-gray-900/60 backdrop-blur px-6 py-3 flex items-center gap-4">
        <div
          className={`w-12 h-12 rounded-2xl flex items-center justify-center text-2xl ring-2 ${accent.ring} bg-gray-800 flex-shrink-0`}
        >
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

        {/* Connection dot */}
        <div className="flex-shrink-0 flex items-center gap-1.5 ml-2">
          <span
            className={`h-2 w-2 rounded-full ${connected ? accent.dot : 'bg-red-500'} ${connected ? 'animate-pulse' : ''}`}
          />
        </div>
      </div>

      {/* Error bar */}
      {error && (
        <div className="px-4 py-2 bg-red-900/30 border-b border-red-800 flex items-center gap-2 text-sm text-red-300">
          <AlertCircle className="h-4 w-4 flex-shrink-0" />
          {error}
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
          </div>
        )}

        {messages.map((msg) => {
          const isUser = msg.role === 'user';
          return (
            <div
              key={msg.id}
              className={`group flex items-end gap-3 ${isUser ? 'flex-row-reverse' : ''}`}
            >
              {/* Avatar */}
              <div
                className={`flex-shrink-0 w-8 h-8 rounded-xl flex items-center justify-center text-base ${
                  isUser ? 'bg-gray-700' : 'bg-gray-800'
                }`}
              >
                {isUser ? '🧑' : emoji}
              </div>

              {/* Bubble */}
              <div className="relative max-w-[72%]">
                <div
                  className={`rounded-2xl px-4 py-3 ${
                    isUser
                      ? `${accent.bubble} text-white rounded-br-sm`
                      : 'bg-gray-800 text-gray-100 border border-gray-700/60 rounded-bl-sm'
                  }`}
                >
                  <p className="text-sm whitespace-pre-wrap break-words leading-relaxed">
                    {msg.content}
                  </p>
                  <p
                    className={`text-[10px] mt-1.5 ${
                      isUser ? 'text-white/50 text-right' : 'text-gray-500'
                    }`}
                  >
                    {msg.timestamp.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
                  </p>
                </div>
                <button
                  onClick={() => handleCopy(msg.id, msg.content)}
                  aria-label="Copy"
                  className="absolute top-1 right-1 opacity-0 group-hover:opacity-100 transition-opacity p-1 rounded bg-gray-700/80 hover:bg-gray-600 text-gray-400 hover:text-white"
                >
                  {copiedId === msg.id ? (
                    <Check className="h-3 w-3 text-green-400" />
                  ) : (
                    <Copy className="h-3 w-3" />
                  )}
                </button>
              </div>
            </div>
          );
        })}

        {/* Typing indicator */}
        {typing && (
          <div className="flex items-end gap-3">
            <div className="flex-shrink-0 w-8 h-8 rounded-xl bg-gray-800 flex items-center justify-center text-base">
              {emoji}
            </div>
            <div className="bg-gray-800 border border-gray-700/60 rounded-2xl rounded-bl-sm px-4 py-3">
              <div className="flex items-center gap-1">
                {[0, 150, 300].map((delay) => (
                  <span
                    key={delay}
                    className="w-2 h-2 bg-gray-400 rounded-full animate-bounce"
                    style={{ animationDelay: `${delay}ms` }}
                  />
                ))}
              </div>
            </div>
          </div>
        )}

        <div ref={messagesEndRef} />
      </div>

      {/* Input */}
      <div className="border-t border-gray-800 bg-gray-900 p-4">
        <div className="flex items-end gap-3 max-w-3xl mx-auto">
          <div className="flex-1">
            <textarea
              ref={inputRef}
              rows={1}
              value={input}
              onChange={handleTextareaChange}
              onKeyDown={handleKeyDown}
              placeholder={connected ? `Message ${persona.name}…` : 'Connecting…'}
              disabled={!connected}
              className="w-full bg-gray-800 border border-gray-700 rounded-2xl px-4 py-3 text-sm text-white placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-offset-0 focus:border-transparent disabled:opacity-50 resize-none overflow-y-auto"
              style={{
                minHeight: '44px',
                maxHeight: '200px',
                // @ts-ignore
                '--tw-ring-color': accent.dot.replace('bg-', ''),
              }}
            />
          </div>
          <button
            onClick={handleSend}
            disabled={!connected || !input.trim()}
            className={`flex-shrink-0 ${accent.bubble} hover:opacity-90 disabled:bg-gray-700 disabled:text-gray-500 text-white rounded-2xl p-3 transition-all`}
          >
            <Send className="h-5 w-5" />
          </button>
        </div>
      </div>
    </div>
  );
}
