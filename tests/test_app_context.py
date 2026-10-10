import importlib.util
from pathlib import Path
import unittest
spec=importlib.util.spec_from_file_location('app_context',Path(__file__).resolve().parents[1]/'desktop/app_context.py')
context=importlib.util.module_from_spec(spec)
spec.loader.exec_module(context)

class AppContextTest(unittest.TestCase):
    def setUp(self):
        self.window=dict(address='0x123',pid=42,at=[-1280,20],size=[900,700],monitor=1,mapped=True,hidden=False,title='Editor')

    def test_title_changes_do_not_invalidate_geometry(self):
        self.assertTrue(context.same_window(self.window,dict(self.window,title='Editor - saved')))

    def test_move_resize_close_focus_and_identity_invalidate(self):
        for update in [dict(at=[0,20]),dict(size=[901,700]),dict(mapped=False),dict(hidden=True),dict(address='0x456'),dict(pid=43),dict(monitor=2)]:
            with self.subTest(update=update):
                self.assertFalse(context.same_window(self.window,dict(self.window,**update)))
        self.assertFalse(context.same_window(self.window,{}))
        self.assertFalse(context.same_window({},{}))
