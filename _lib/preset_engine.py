# -*- coding: utf-8 -*-
"""
==============================================================================
UNIVERSAL ADAPTIVE PRESET ENGINE v1.0
Universal Adaptive Presets & User History Substitution Module
==============================================================================
"""

import time
from typing import List, Dict, Any, Optional, Callable


class AdaptivePresetItem:
    """Represents a single preset slot or candidate value."""
    def __init__(
        self,
        value: Any,
        label: str = "",
        is_default: bool = False,
        is_pinned: bool = False,
        count: int = 1,
        last_used: float = 0.0,
        extra: Optional[Dict[str, Any]] = None
    ):
        self.value = value
        self.label = label
        self.is_default = is_default
        self.is_pinned = is_pinned
        self.count = count
        self.last_used = last_used or time.time()
        self.extra = extra or {}

    def to_dict(self) -> Dict[str, Any]:
        return {
            "value": self.value,
            "label": self.label,
            "is_default": self.is_default,
            "is_pinned": self.is_pinned,
            "count": self.count,
            "last_used": self.last_used,
            "extra": self.extra,
        }

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "AdaptivePresetItem":
        return cls(
            value=data.get("value"),
            label=data.get("label", ""),
            is_default=data.get("is_default", False),
            is_pinned=data.get("is_pinned", False),
            count=data.get("count", 1),
            last_used=data.get("last_used", 0.0),
            extra=data.get("extra", {})
        )


class AdaptivePresetDomain:
    """
    Manages a single domain/category of presets (e.g. 'alarm_time', 'timer_duration').
    """
    def __init__(
        self,
        domain_key: str,
        default_items: List[Dict[str, Any]],
        max_slots: int = 3,
        label_formatter: Optional[Callable[[Any], str]] = None,
        recency_weight: float = 2.0,
        frequency_weight: float = 1.0
    ):
        self.domain_key = domain_key
        self.max_slots = max(1, max_slots)
        self.label_formatter = label_formatter or (lambda v: str(v))
        self.recency_weight = recency_weight
        self.frequency_weight = frequency_weight
        
        self.default_items: List[AdaptivePresetItem] = []
        for d in default_items:
            val = d.get("value")
            lbl = d.get("label") or self.label_formatter(val)
            self.default_items.append(
                AdaptivePresetItem(
                    value=val,
                    label=lbl,
                    is_default=True,
                    is_pinned=d.get("is_pinned", False),
                    count=d.get("count", 1),
                    extra=d.get("extra", {})
                )
            )

        self.history: Dict[str, AdaptivePresetItem] = {}

    def record_usage(self, value: Any, label: str = "", extra: Optional[Dict[str, Any]] = None):
        """Records a user choice, boosting its ranking score."""
        key = str(value)
        now = time.time()
        if key in self.history:
            item = self.history[key]
            item.count += 1
            item.last_used = now
            if label:
                item.label = label
            if extra:
                item.extra.update(extra)
        else:
            lbl = label or self.label_formatter(value)
            matching_def = next((d for d in self.default_items if str(d.value) == key), None)
            is_def = matching_def is not None
            is_pinned = matching_def.is_pinned if matching_def else False
            
            self.history[key] = AdaptivePresetItem(
                value=value,
                label=lbl,
                is_default=is_def,
                is_pinned=is_pinned,
                count=1,
                last_used=now,
                extra=extra or {}
            )

    def compute_score(self, item: AdaptivePresetItem, now: Optional[float] = None) -> float:
        """Calculates combined score based on count and recency."""
        if now is None:
            now = time.time()
        
        if item.is_pinned:
            return 999999.0 + item.count

        hours_ago = max(0.0, (now - item.last_used) / 3600.0)
        recency_factor = self.recency_weight / (1.0 + (hours_ago / 24.0))
        freq_factor = item.count * self.frequency_weight
        return freq_factor + recency_factor

    def get_active_presets(self, custom_formatter: Optional[Callable[[AdaptivePresetItem], str]] = None) -> List[AdaptivePresetItem]:
        """
        Returns the top `max_slots` preset items, automatically substituting defaults
        with high-scoring user habits.
        """
        now = time.time()
        candidates: List[AdaptivePresetItem] = []
        seen_keys = set()

        for k, item in self.history.items():
            candidates.append(item)
            seen_keys.add(k)

        for d in self.default_items:
            k = str(d.value)
            if k not in seen_keys:
                candidates.append(d)
                seen_keys.add(k)

        candidates.sort(key=lambda it: self.compute_score(it, now), reverse=True)
        result = candidates[:self.max_slots]

        if custom_formatter:
            for item in result:
                formatted = custom_formatter(item)
                if formatted:
                    item.label = formatted
        return result

    def pin_preset(self, value: Any, pinned: bool = True):
        """Locks a preset to always appear in active slots."""
        key = str(value)
        if key in self.history:
            self.history[key].is_pinned = pinned
        else:
            matching_def = next((d for d in self.default_items if str(d.value) == key), None)
            if matching_def:
                matching_def.is_pinned = pinned
            else:
                self.record_usage(value)
                if key in self.history:
                    self.history[key].is_pinned = pinned

    def reset_to_defaults(self):
        """Clears user history and reverts to pure factory defaults."""
        self.history.clear()

    def serialize(self) -> Dict[str, Any]:
        return {
            "max_slots": self.max_slots,
            "history": {k: item.to_dict() for k, item in self.history.items()}
        }

    def deserialize(self, data: Dict[str, Any]):
        if not isinstance(data, dict):
            return
        self.max_slots = data.get("max_slots", self.max_slots)
        hist_data = data.get("history", {})
        if isinstance(hist_data, dict):
            for k, it_data in hist_data.items():
                try:
                    self.history[k] = AdaptivePresetItem.from_dict(it_data)
                except Exception:
                    pass


class PresetEngine:
    """
    Universal Preset Engine managing multiple domains for an application.
    """
    def __init__(self, config_store: Optional[Any] = None, namespace: str = "adaptive_presets"):
        self.config_store = config_store
        self.namespace = namespace
        self.domains: Dict[str, AdaptivePresetDomain] = {}
        self._load_from_config()

    def register_domain(
        self,
        domain_key: str,
        default_items: List[Dict[str, Any]],
        max_slots: int = 3,
        label_formatter: Optional[Callable[[Any], str]] = None
    ) -> AdaptivePresetDomain:
        """Registers and initializes a preset domain."""
        domain = AdaptivePresetDomain(
            domain_key=domain_key,
            default_items=default_items,
            max_slots=max_slots,
            label_formatter=label_formatter
        )
        self.domains[domain_key] = domain
        
        if self.config_store:
            cfg_data = self.config_store.get(self.namespace, {})
            if isinstance(cfg_data, dict) and domain_key in cfg_data:
                domain.deserialize(cfg_data[domain_key])
                
        return domain

    def record_usage(self, domain_key: str, value: Any, label: str = "", extra: Optional[Dict[str, Any]] = None):
        """Records a user selection in the specified domain and saves config."""
        if domain_key in self.domains:
            self.domains[domain_key].record_usage(value, label, extra)
            self._save_to_config()

    def get_active_presets(self, domain_key: str, custom_formatter: Optional[Callable[[AdaptivePresetItem], str]] = None) -> List[AdaptivePresetItem]:
        """Gets the active top presets for the domain."""
        if domain_key in self.domains:
            return self.domains[domain_key].get_active_presets(custom_formatter)
        return []

    def reset_domain(self, domain_key: str):
        """Resets a domain to factory defaults."""
        if domain_key in self.domains:
            self.domains[domain_key].reset_to_defaults()
            self._save_to_config()

    def _load_from_config(self):
        if not self.config_store:
            return
        cfg_data = self.config_store.get(self.namespace, {})
        if isinstance(cfg_data, dict):
            for dom_key, dom_data in cfg_data.items():
                if dom_key in self.domains:
                    self.domains[dom_key].deserialize(dom_data)

    def _save_to_config(self):
        if not self.config_store:
            return
        data = {k: dom.serialize() for k, dom in self.domains.items()}
        self.config_store.set(self.namespace, data)
        if hasattr(self.config_store, "save"):
            try:
                self.config_store.save()
            except Exception:
                pass


_global_engine: Optional[PresetEngine] = None

def get_preset_engine(config_store: Optional[Any] = None) -> PresetEngine:
    """Returns the global PresetEngine instance, initialized with config_store."""
    global _global_engine
    if _global_engine is None:
        _global_engine = PresetEngine(config_store=config_store)
    elif config_store and _global_engine.config_store is None:
        _global_engine.config_store = config_store
        _global_engine._load_from_config()
    return _global_engine
