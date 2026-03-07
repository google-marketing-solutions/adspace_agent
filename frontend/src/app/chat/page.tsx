"use client";

import { useState, useRef, useEffect, useCallback } from "react";
import { useMutation } from "@tanstack/react-query";
import {
  Send,
  Bot,
  User,
  Loader2,
  RotateCcw,
  Copy,
  Check,
  Sparkles,
  TrendingUp,
  Search,
  Zap,
  MessageSquare,
  BarChart3,
} from "lucide-react";
import { useToast } from "@/hooks/use-toast";
import { useAccount } from "@/components/AccountContext";
import { sendChatMessage } from "@/lib/api";
import type { ChatMessage } from "@/lib/types";
import { Card, CardContent } from "@/components/ui/card";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Avatar, AvatarFallback } from "@/components/ui/avatar";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import { Textarea } from "@/components/ui/textarea";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";

export default function ChatPage() {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState("");
  const [sessionId, setSessionId] = useState<string | undefined>();
  const [copiedIdx, setCopiedIdx] = useState<number | null>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const { addToast } = useToast();
  const { customerId } = useAccount();

  const scrollToBottom = useCallback(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, []);

  useEffect(() => {
    scrollToBottom();
  }, [messages, scrollToBottom]);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const chatMutation = useMutation({
    mutationFn: (message: string) => sendChatMessage(message, sessionId, customerId),
    onSuccess: (data) => {
      setSessionId(data.session_id);
      setMessages((prev) => [
        ...prev,
        { role: "assistant", content: data.response, timestamp: new Date().toISOString() },
      ]);
    },
    onError: (error: Error) => {
      setMessages((prev) => [
        ...prev,
        { role: "assistant", content: `Error: ${error.message}`, timestamp: new Date().toISOString() },
      ]);
      addToast("error", "Chat Error", error.message);
    },
  });

  const handleSend = () => {
    if (!input.trim() || chatMutation.isPending) return;
    const userMessage: ChatMessage = { role: "user", content: input.trim(), timestamp: new Date().toISOString() };
    setMessages((prev) => [...prev, userMessage]);
    chatMutation.mutate(input.trim());
    setInput("");
    if (inputRef.current) inputRef.current.style.height = "auto";
  };

  const handleNewChat = () => {
    setMessages([]);
    setSessionId(undefined);
    setInput("");
    inputRef.current?.focus();
    addToast("info", "New conversation", "Chat history has been cleared.");
  };

  const handleCopy = (text: string, idx: number) => {
    navigator.clipboard.writeText(text);
    setCopiedIdx(idx);
    setTimeout(() => setCopiedIdx(null), 2000);
  };

  const handleTextareaInput = () => {
    if (inputRef.current) {
      inputRef.current.style.height = "auto";
      inputRef.current.style.height = Math.min(inputRef.current.scrollHeight, 150) + "px";
    }
  };

  const suggestedPrompts = [
    { text: "Show me my top performing campaigns", icon: TrendingUp, color: "text-emerald-600 dark:text-emerald-400 bg-emerald-50 dark:bg-emerald-950" },
    { text: "What keywords have the highest CPC?", icon: Search, color: "text-blue-600 dark:text-blue-400 bg-blue-50 dark:bg-blue-950" },
    { text: "Generate optimization recommendations", icon: Sparkles, color: "text-violet-600 dark:text-violet-400 bg-violet-50 dark:bg-violet-950" },
    { text: "What's my overall ROAS this month?", icon: BarChart3, color: "text-amber-600 dark:text-amber-400 bg-amber-50 dark:bg-amber-950" },
    { text: "Which campaigns should I pause?", icon: Zap, color: "text-red-600 dark:text-red-400 bg-red-50 dark:bg-red-950" },
    { text: "Analyze my ad copy performance", icon: MessageSquare, color: "text-pink-600 dark:text-pink-400 bg-pink-50 dark:bg-pink-950" },
  ];

  return (
    <TooltipProvider>
      <div className="flex flex-col h-[calc(100vh-5rem)]">
        {/* Header */}
        <div className="shrink-0 flex items-center justify-between pb-4 border-b border-border">
          <div className="flex items-center gap-3">
            <Avatar className="h-10 w-10 bg-gradient-to-br from-indigo-500 to-violet-600 shadow-lg">
              <AvatarFallback className="bg-transparent text-white">
                <Bot className="w-5 h-5" />
              </AvatarFallback>
            </Avatar>
            <div>
              <h1 className="text-xl font-bold text-foreground">AI Assistant</h1>
              <p className="text-xs text-muted-foreground">
                Google Ads optimization powered by Gemini
                {sessionId && (
                  <span className="ml-2 inline-flex items-center gap-1">
                    <span className="w-1.5 h-1.5 rounded-full bg-emerald-500 animate-pulse" />
                    <span className="text-emerald-600 dark:text-emerald-400">Session active</span>
                  </span>
                )}
              </p>
            </div>
          </div>
          {messages.length > 0 && (
            <Button variant="outline" size="sm" onClick={handleNewChat}>
              <RotateCcw className="w-3.5 h-3.5 mr-2" />
              New Chat
            </Button>
          )}
        </div>

        {/* Messages */}
        <ScrollArea className="flex-1 mt-4 pb-4">
          <div className="space-y-5">
            {messages.length === 0 && (
              <div className="flex flex-col items-center justify-center h-full min-h-[400px] text-center animate-fade-in">
                <Avatar className="h-20 w-20 bg-gradient-to-br from-indigo-500 via-violet-500 to-purple-600 mb-6 shadow-xl">
                  <AvatarFallback className="bg-transparent text-white">
                    <Sparkles className="w-10 h-10" />
                  </AvatarFallback>
                </Avatar>
                <h3 className="text-xl font-bold text-foreground">How can I help you today?</h3>
                <p className="text-sm text-muted-foreground mt-2 max-w-md leading-relaxed">
                  I can analyze your Google Ads performance, generate recommendations,
                  make changes to campaigns, and more.
                </p>
                <div className="grid grid-cols-2 lg:grid-cols-3 gap-3 mt-8 max-w-2xl">
                  {suggestedPrompts.map((prompt) => {
                    const Icon = prompt.icon;
                    return (
                      <Card
                        key={prompt.text}
                        className="group cursor-pointer hover:shadow-md transition-all duration-200"
                        onClick={() => { setInput(prompt.text); inputRef.current?.focus(); }}
                      >
                        <CardContent className="p-3.5 text-left">
                          <div className={`w-8 h-8 rounded-lg ${prompt.color} flex items-center justify-center mb-2 group-hover:scale-110 transition-transform`}>
                            <Icon className="w-4 h-4" />
                          </div>
                          <span className="text-sm text-muted-foreground group-hover:text-foreground transition-colors">{prompt.text}</span>
                        </CardContent>
                      </Card>
                    );
                  })}
                </div>
              </div>
            )}

            {messages.map((msg, i) => (
              <div key={i} className={`flex gap-3 animate-fade-in ${msg.role === "user" ? "justify-end" : "justify-start"}`}>
                {msg.role === "assistant" && (
                  <Avatar className="h-8 w-8 bg-gradient-to-br from-indigo-500 to-violet-600 shadow-sm shrink-0">
                    <AvatarFallback className="bg-transparent text-white">
                      <Bot className="w-4 h-4" />
                    </AvatarFallback>
                  </Avatar>
                )}
                <div className="group relative max-w-[70%]">
                  <div className={`min-w-[60px] rounded-2xl px-4 py-3 text-sm leading-relaxed ${
                    msg.role === "user"
                      ? "bg-gradient-to-r from-indigo-600 to-violet-600 text-white shadow-lg"
                      : "bg-card border border-border text-foreground shadow-sm"
                  }`}>
                    <div className="whitespace-pre-wrap break-words">{msg.content}</div>
                    {msg.timestamp && (
                      <p className={`text-xs mt-1.5 ${msg.role === "user" ? "text-indigo-200" : "text-muted-foreground"}`}>
                        {new Date(msg.timestamp).toLocaleTimeString()}
                      </p>
                    )}
                  </div>
                  {msg.role === "assistant" && (
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <button
                          onClick={() => handleCopy(msg.content, i)}
                          className="absolute -bottom-1 right-2 opacity-0 group-hover:opacity-100 transition-all bg-card border border-border rounded-lg p-1.5 shadow-sm hover:bg-muted"
                        >
                          {copiedIdx === i ? (
                            <Check className="w-3 h-3 text-emerald-500" />
                          ) : (
                            <Copy className="w-3 h-3 text-muted-foreground" />
                          )}
                        </button>
                      </TooltipTrigger>
                      <TooltipContent>{copiedIdx === i ? "Copied!" : "Copy message"}</TooltipContent>
                    </Tooltip>
                  )}
                </div>
                {msg.role === "user" && (
                  <Avatar className="h-8 w-8 shrink-0">
                    <AvatarFallback>
                      <User className="w-4 h-4" />
                    </AvatarFallback>
                  </Avatar>
                )}
              </div>
            ))}

            {chatMutation.isPending && (
              <div className="flex gap-3 animate-fade-in">
                <Avatar className="h-8 w-8 bg-gradient-to-br from-indigo-500 to-violet-600 shadow-sm shrink-0">
                  <AvatarFallback className="bg-transparent text-white">
                    <Bot className="w-4 h-4" />
                  </AvatarFallback>
                </Avatar>
                <div className="bg-card border border-border rounded-2xl px-4 py-3 shadow-sm">
                  <div className="flex items-center gap-2">
                    <div className="flex gap-1">
                      <span className="w-2 h-2 bg-primary/60 rounded-full animate-bounce" style={{ animationDelay: "0ms" }} />
                      <span className="w-2 h-2 bg-primary/60 rounded-full animate-bounce" style={{ animationDelay: "150ms" }} />
                      <span className="w-2 h-2 bg-primary/60 rounded-full animate-bounce" style={{ animationDelay: "300ms" }} />
                    </div>
                    <span className="text-sm text-muted-foreground ml-1">Thinking...</span>
                  </div>
                </div>
              </div>
            )}

            <div ref={messagesEndRef} />
          </div>
        </ScrollArea>

        {/* Input */}
        <div className="shrink-0 pt-4 border-t border-border">
          <Card className="shadow-lg">
            <CardContent className="flex items-end gap-2 p-2">
              <textarea
                ref={inputRef}
                value={input}
                onChange={(e) => setInput(e.target.value)}
                onInput={handleTextareaInput}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && !e.shiftKey) {
                    e.preventDefault();
                    handleSend();
                  }
                }}
                placeholder="Ask anything about your Google Ads..."
                className="flex-1 resize-none border-0 bg-transparent px-3 py-2.5 text-sm focus:outline-none placeholder:text-muted-foreground/50 max-h-[150px]"
                rows={1}
                disabled={chatMutation.isPending}
              />
              <button
                onClick={handleSend}
                disabled={!input.trim() || chatMutation.isPending}
                className="bg-gradient-to-r from-indigo-600 to-violet-600 text-white rounded-xl p-2.5 hover:shadow-lg transition-all disabled:opacity-30 disabled:cursor-not-allowed shrink-0"
              >
                {chatMutation.isPending ? (
                  <Loader2 className="w-5 h-5 animate-spin" />
                ) : (
                  <Send className="w-5 h-5" />
                )}
              </button>
            </CardContent>
          </Card>
          <p className="text-xs text-muted-foreground/50 mt-2 text-center">
            AI responses are generated by Gemini and may not always be accurate. Always verify before making changes.
          </p>
        </div>
      </div>
    </TooltipProvider>
  );
}
