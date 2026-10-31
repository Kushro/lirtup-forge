"""
Parsing of MagpieFX effect selections from the config and the CLI.

An effect chain is a list whose items are either:

* a string ``"Name"`` or ``"Name:param=value,param=value"`` (CLI form), or
* a mapping ``{"name": "Name", "params": {"param": value}}`` (YAML form).

``Name`` is a bundled/user effect name or a path to a ``.hlsl`` file.
"""

from typing import Any, Dict, Iterable, List, Tuple

EffectSpec = Tuple[str, Dict[str, float]]


def parse_effect_spec(item: Any) -> EffectSpec:
    """Normalise one chain item to ``(name, params)``; raises ValueError."""
    if isinstance(item, dict):
        name = item.get("name")
        params = item.get("params") or {}
        if not isinstance(name, str) or not name:
            raise ValueError(f"MagpieFX effect entry needs a 'name': {item!r}")
        if not isinstance(params, dict):
            raise ValueError(f"'params' of effect '{name}' must be a mapping")
        unknown = set(item) - {"name", "params"}
        if unknown:
            raise ValueError(f"Unknown keys in effect '{name}': {sorted(unknown)}")
        try:
            return name, {str(k): float(v) for k, v in params.items()}
        except (TypeError, ValueError):
            raise ValueError(f"Parameters of effect '{name}' must be numbers") from None

    if not isinstance(item, str) or not item.strip():
        raise ValueError(f"Invalid MagpieFX effect entry: {item!r}")

    # A path may contain ':' only in unusual names; split on the last ':'
    # that is followed by 'key=' pairs.
    name, sep, rest = item.strip().rpartition(":")
    if not sep or "=" not in rest:
        return item.strip(), {}
    params: Dict[str, float] = {}
    for pair in rest.split(","):
        key, eq, value = pair.partition("=")
        if not eq or not key.strip():
            raise ValueError(f"Invalid parameter '{pair}' in effect '{name}'")
        try:
            params[key.strip()] = float(value)
        except ValueError:
            raise ValueError(
                f"Parameter '{key.strip()}' of effect '{name}' must be a number"
            ) from None
    return name, params


def parse_effect_chain(items: Iterable[Any]) -> List[EffectSpec]:
    if isinstance(items, (str, dict)):
        items = [items]
    return [parse_effect_spec(i) for i in items]
