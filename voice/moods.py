import re

TAG = re.compile(r"\[([A-Za-z][\w ]*)\]")


def pieces(text: str, moods) -> list[tuple[str, str]]:
    out, mood, pos = [], "", 0
    for m in TAG.finditer(text):
        out.append((mood, text[pos:m.start()]))
        tag = m.group(1).strip().lower()
        mood = tag if tag in moods else mood
        pos = m.end()
    out.append((mood, text[pos:]))
    return [(m, " ".join(w.split())) for m, w in out if w.strip()]


if __name__ == "__main__":
    known = {"smug", "excited"}
    assert pieces("Hi there.", known) == [("", "Hi there.")]
    assert pieces("[smug] Cute. [excited] We did it!", known) == [("smug", "Cute."), ("excited", "We did it!")]
    assert pieces("Fine. [laughs] [Smug] Bold choice.", known) == [("", "Fine."), ("smug", "Bold choice.")]
    assert pieces("[excited]", known) == []
    print("ok")
