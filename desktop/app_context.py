"""Window identity/geometry checks; UI content changes need separate verification."""
def same_window(expected, current):
    return (
        isinstance(expected, dict) and isinstance(current, dict)
        and bool(expected.get('address')) and bool(expected.get('pid'))
        and current.get('mapped') is True and not current.get('hidden', False)
        and all(expected.get(key) == current.get(key)
                for key in ('address', 'pid', 'at', 'size', 'monitor'))
    )
