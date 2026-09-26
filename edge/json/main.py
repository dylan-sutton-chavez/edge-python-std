from _json import dumps as _dumps, loads as _loads

def dumps(value, **options):
    return _dumps(value, **options)

def loads(text, **hooks):
    return _loads(text, **hooks)
