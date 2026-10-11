#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
RuhI (如喜) Western Lexicon Generator
Extracts high-frequency English words from ee9_en corpus and standard dictionaries,
generating ruhi_lexicon/data/lexicon_english.tsv.
Format: word \t frequency \t pos_tag
"""

import os
import re
import math
from collections import Counter

OUTPUT_TSV = r"C:\ATGprjs\ruhi\ruhi_lexicon\data\lexicon_english.tsv"
CORPUS_DIR = r"c:\ATGprjs\RuhOS\corpus\ee9_en"

# Common base core English words (function words, tech terms, high-frequency nouns/verbs/adjectives)
CORE_WORDS = [
    # Top function & grammatical words
    "the", "be", "to", "of", "and", "a", "in", "that", "have", "i", "it", "for", "not", "on", "with",
    "he", "as", "you", "do", "at", "this", "but", "his", "by", "from", "they", "we", "say", "her",
    "she", "or", "an", "will", "my", "one", "all", "would", "there", "their", "what", "so", "up",
    "out", "if", "about", "who", "get", "which", "go", "me", "when", "make", "can", "like", "time",
    "no", "just", "him", "know", "take", "people", "into", "year", "your", "good", "some", "could",
    "them", "see", "other", "than", "then", "now", "look", "only", "come", "its", "over", "think",
    "also", "back", "after", "use", "two", "how", "our", "work", "first", "well", "way", "even",
    "new", "want", "because", "any", "these", "give", "day", "most", "us", "is", "am", "are", "was",
    "were", "been", "has", "had", "did", "does", "done", "having",
    # Demonstratives, prepositions, adverbs
    "here", "where", "why", "again", "off", "away", "near", "far", "always", "never", "often", "sometimes",
    "today", "tomorrow", "yesterday", "tonight", "before", "while", "during", "since", "until", "under",
    "above", "below", "between", "through", "against", "without", "within", "along", "across", "behind",
    # Common verbs
    "read", "write", "speak", "talk", "hear", "listen", "run", "walk", "jump", "fly", "drive", "ride",
    "sit", "stand", "sleep", "wake", "eat", "drink", "buy", "sell", "pay", "cost", "spend", "build",
    "create", "design", "develop", "code", "debug", "test", "run", "start", "stop", "pause", "resume",
    "open", "close", "save", "delete", "remove", "add", "edit", "update", "modify", "insert", "select",
    "find", "search", "replace", "copy", "cut", "paste", "undo", "redo", "print", "export", "import",
    "send", "receive", "connect", "disconnect", "download", "upload", "install", "uninstall", "load",
    "help", "support", "learn", "teach", "study", "remember", "forget", "understand", "believe", "hope",
    # Common nouns
    "book", "sample", "example", "word", "sentence", "text", "file", "folder", "directory", "path",
    "system", "computer", "screen", "keyboard", "mouse", "monitor", "window", "desktop", "laptop",
    "network", "server", "client", "internet", "website", "page", "browser", "document", "data",
    "database", "table", "column", "row", "record", "field", "query", "index", "key", "value",
    "code", "program", "software", "hardware", "application", "app", "tool", "utility", "script",
    "language", "function", "method", "class", "object", "type", "string", "number", "boolean",
    "array", "list", "map", "set", "tree", "graph", "node", "edge", "queue", "stack", "buffer",
    "memory", "disk", "storage", "cpu", "gpu", "device", "driver", "kernel", "process", "thread",
    "user", "account", "password", "login", "logout", "admin", "profile", "setting", "preference",
    "configuration", "config", "option", "feature", "version", "release", "build", "commit", "branch",
    "repository", "repo", "project", "workspace", "terminal", "console", "command", "prompt",
    "message", "error", "warning", "info", "notice", "alert", "notification", "event", "signal",
    "problem", "issue", "bug", "fix", "patch", "solution", "result", "output", "input", "response",
    "request", "header", "body", "status", "code", "token", "session", "cookie", "cache", "api",
    "friend", "family", "mother", "father", "parent", "child", "son", "daughter", "brother", "sister",
    "man", "woman", "person", "human", "life", "world", "earth", "country", "city", "state", "place",
    "house", "home", "room", "door", "car", "train", "plane", "ship", "water", "food", "air", "light",
    # Common adjectives
    "small", "big", "large", "huge", "tiny", "great", "high", "low", "long", "short", "tall", "wide",
    "narrow", "thick", "thin", "heavy", "light", "dark", "bright", "clear", "clean", "dirty", "fresh",
    "hot", "cold", "warm", "cool", "dry", "wet", "fast", "quick", "slow", "hard", "soft", "easy",
    "simple", "complex", "difficult", "tough", "strong", "weak", "rich", "poor", "young", "old",
    "happy", "sad", "good", "bad", "true", "false", "real", "fake", "right", "wrong", "correct",
    "important", "necessary", "possible", "impossible", "ready", "able", "available", "active",
    "open", "closed", "public", "private", "protected", "static", "dynamic", "constant", "variable",
    "normal", "regular", "special", "unique", "different", "same", "similar", "equal", "free",
    # Tech / Brand / Modern terms
    "google", "apple", "microsoft", "windows", "linux", "macos", "android", "ios", "rust", "python",
    "javascript", "typescript", "html", "css", "json", "yaml", "xml", "sql", "git", "github",
    "gitlab", "docker", "colab", "gemini", "gemma", "ollama", "openai", "claude", "meta", "nvidia",
    "intel", "amd", "arm", "wifi", "bluetooth", "usb", "hdmi", "gpu", "cpu", "ssd", "ram", "rom",
    "url", "http", "https", "tcp", "udp", "ip", "dns", "ssh", "ssl", "tls", "api", "sdk", "cli", "gui",
    # Days & Months
    "monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday",
    "january", "february", "march", "april", "may", "june", "july", "august", "september", "october",
    "november", "december"
]

def clean_word(token: str) -> str:
    t = token.strip(" \t\r\n.,!?:;\"'()[]{}<>-/*_#`~@$%^&+=|\\")
    if re.match(r"^[a-zA-Z]+(?:'[a-zA-Z]+)?$", t):
        return t.lower()
    return ""

def main():
    counts = Counter()
    
    # 1. Initialize core vocabulary with high base counts
    for word in CORE_WORDS:
        counts[word.lower()] = 1000

    # 2. Extract words from corpus if available
    if os.path.exists(CORPUS_DIR):
        print(f"Scanning corpus: {CORPUS_DIR}...")
        file_count = 0
        for root, _, files in os.walk(CORPUS_DIR):
            for f in files:
                if f.endswith(".txt"):
                    file_count += 1
                    path = os.path.join(root, f)
                    try:
                        with open(path, "r", encoding="utf-8", errors="ignore") as fp:
                            for line in fp:
                                for tok in line.split():
                                    w = clean_word(tok)
                                    if w and len(w) >= 1 and len(w) <= 30:
                                        if len(w) == 1 and w not in ('a', 'i'):
                                            continue
                                        counts[w] += 1
                    except Exception as e:
                        pass
        print(f"Processed {file_count} text files.")

    print(f"Total vocabulary size: {len(counts)}")

    # Sort by frequency descending
    sorted_words = counts.most_common()

    # Write to TSV
    os.makedirs(os.path.dirname(OUTPUT_TSV), exist_ok=True)
    with open(OUTPUT_TSV, "w", encoding="utf-8") as out:
        out.write("# RuhI Western Lexicon TSV\n")
        out.write("# word\tfrequency\tlog_weight\n")
        max_freq = sorted_words[0][1] if sorted_words else 1
        for word, freq in sorted_words:
            # Normalized log-odds weight (0.0 to 100.0)
            log_w = round(math.log(freq + 1) * 8.5, 2)
            out.write(f"{word}\t{freq}\t{log_w}\n")

    print(f"Generated Western Lexicon: {OUTPUT_TSV} ({len(sorted_words)} entries)")

if __name__ == "__main__":
    main()
