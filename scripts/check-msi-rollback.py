#!/usr/bin/env python3
"""Require executed deferred failure and transactional rollback in an MSI log."""
import re
import sys
from pathlib import Path


def verify(text):
    patterns = [
        r'Action start[^\r\n]*: RemoveExistingProducts\.',
        r'Executing op: FileCopy\(',
        r'Executing op: CustomActionSchedule\(Action=Wix4FailWhenDeferred_X64,',
        r'CustomAction Wix4FailWhenDeferred_X64 returned actual error code 1603',
        r'Executing op: Header\([^\r\n]*ScriptType=2[,)]',
    ]
    position = 0
    for pattern in patterns:
        match = re.search(pattern, text[position:])
        if match is None:
            raise ValueError('missing or out-of-order MSI rollback execution evidence: ' + pattern)
        position += match.end()
    # Fault-bearing MSI transaction must finish as a failure, not a swallowed CA.
    if not re.search(r'Action ended[^\r\n]*: InstallFinalize\. Return value 3\.', text):
        raise ValueError('missing failed InstallFinalize result')
    return True


if __name__ == '__main__':
    data = Path(sys.argv[1]).read_bytes()
    text = data.decode('utf-16') if data.startswith((b'\xff\xfe', b'\xfe\xff')) else data.decode('utf-8-sig')
    verify(text)
    print('MSI_DEFERRED_FAILURE_AND_ROLLBACK_SEQUENCE_VERIFIED')
