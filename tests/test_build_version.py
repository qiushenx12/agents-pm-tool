"""build.py 的交互与版本状态测试；不会触发正式打包。"""

import io
import unittest
from contextlib import redirect_stdout
from pathlib import Path
from unittest.mock import patch

import build


def published_state():
    return {
        "schemaVersion": 1,
        "currentVersion": "1.0.9",
        "published": True,
        "releases": [{"version": "1.0.9"}],
    }


class BuildVersionTests(unittest.TestCase):
    def test_invalid_version_reprompts_until_a_new_valid_version_is_entered(self):
        state = published_state()
        with patch(
            "builtins.input",
            side_effect=["bad", "1.0.10", "18446744073709551616.0.0", "1.0.9", "1.1.0"],
        ) as prompt:
            self.assertEqual(build.prompt_build_version(state), "1.1.0")
        self.assertEqual(prompt.call_count, 5)
        self.assertEqual(state, published_state())

    def test_enter_keeps_even_a_published_version_without_auto_increment(self):
        state = published_state()
        with patch("builtins.input", return_value=""):
            selected = build.prompt_build_version(state)
        with patch.object(build, "sync_project_versions") as sync, patch.object(
            build, "save_version_state"
        ) as save:
            version, prepared = build.prepare_build_version(state, selected)
        self.assertEqual(version, "1.0.9")
        self.assertEqual(prepared, state)
        self.assertTrue(prepared["published"])
        sync.assert_called_once_with("1.0.9")
        save.assert_not_called()

    def test_new_version_is_synced_and_marked_unpublished_before_build(self):
        state = published_state()
        with patch.object(build, "sync_project_versions") as sync, patch.object(
            build, "save_version_state"
        ) as save:
            version, prepared = build.prepare_build_version(state, "1.1.0")
        self.assertEqual(version, "1.1.0")
        self.assertEqual(prepared["currentVersion"], "1.1.0")
        self.assertFalse(prepared["published"])
        self.assertEqual(state, published_state())
        sync.assert_called_once_with("1.1.0")
        save.assert_called_once_with(prepared)

    def test_invalid_direct_version_does_not_sync_files(self):
        with patch.object(build, "sync_project_versions") as sync:
            with self.assertRaises(build.VersionStateError):
                build.prepare_build_version(published_state(), "1.0.9")
        sync.assert_not_called()

    def test_main_asks_for_version_before_build_and_pauses_after_success(self):
        events = []

        def answer(message):
            events.append("exit_prompt" if "退出" in message else "version_prompt")
            return "" if "退出" in message else "1.1.0"

        with patch.object(build, "PLATFORM_KEY", "windows"), patch.object(
            build, "PLATFORM_LABEL", "Windows"
        ), patch.object(build, "load_version_state", return_value=published_state()), patch.object(
            build, "sync_project_versions"
        ), patch.object(build, "save_version_state"), patch.object(
            build, "check_npm", return_value=True
        ), patch.object(build, "check_rust", return_value=True), patch.object(
            build, "install_deps", return_value=True
        ), patch.object(build, "load_product_name", return_value="Agents PM Tool"), patch.object(
            build, "run_build", side_effect=lambda *_: events.append("build") or True
        ), patch.object(
            build,
            "find_built_artifacts",
            return_value=[Path("Agents PM Tool_1.1.0_x64-setup.exe")],
        ), patch.object(build, "record_released") as record, patch(
            "builtins.input", side_effect=answer
        ) as prompt:
            self.assertEqual(build.main(), 0)
        self.assertEqual(events, ["version_prompt", "build", "exit_prompt"])
        self.assertEqual(prompt.call_count, 2)
        record.assert_called_once_with(
            "1.1.0", [Path("Agents PM Tool_1.1.0_x64-setup.exe")]
        )

    def test_main_rebuilds_the_published_version_without_another_release(self):
        with patch.object(build, "PLATFORM_KEY", "windows"), patch.object(
            build, "PLATFORM_LABEL", "Windows"
        ), patch.object(build, "load_version_state", return_value=published_state()), patch.object(
            build, "sync_project_versions"
        ), patch.object(build, "save_version_state") as save, patch.object(
            build, "check_npm", return_value=True
        ), patch.object(build, "check_rust", return_value=True), patch.object(
            build, "install_deps", return_value=True
        ), patch.object(build, "load_product_name", return_value="Agents PM Tool"), patch.object(
            build, "run_build", return_value=True
        ) as run_build, patch.object(
            build,
            "find_built_artifacts",
            return_value=[Path("Agents PM Tool_1.0.9_x64-setup.exe")],
        ), patch.object(build, "record_released") as record, patch(
            "builtins.input", side_effect=["", ""]
        ) as prompt:
            self.assertEqual(build.main(), 0)
        self.assertEqual(prompt.call_count, 2)
        self.assertIn("退出", prompt.call_args_list[1].args[0])
        run_build.assert_called_once_with("1.0.9", "Agents PM Tool")
        save.assert_not_called()
        record.assert_not_called()

    def test_failed_build_never_records_a_release(self):
        with patch.object(build, "PLATFORM_KEY", "windows"), patch.object(
            build, "PLATFORM_LABEL", "Windows"
        ), patch.object(build, "load_version_state", return_value=published_state()), patch.object(
            build, "sync_project_versions"
        ), patch.object(build, "save_version_state"), patch.object(
            build, "check_npm", return_value=True
        ), patch.object(build, "check_rust", return_value=True), patch.object(
            build, "install_deps", return_value=True
        ), patch.object(build, "load_product_name", return_value="Agents PM Tool"), patch.object(
            build, "run_build", return_value=False
        ), patch.object(build, "record_released") as record, patch(
            "builtins.input", side_effect=["1.1.0", ""]
        ) as prompt:
            output = io.StringIO()
            with redirect_stdout(output):
                self.assertEqual(build.main(), 1)
        record.assert_not_called()
        self.assertIn("打包失败", output.getvalue())
        self.assertEqual(prompt.call_count, 2)
        self.assertIn("退出", prompt.call_args_list[1].args[0])

    def test_build_command_exception_is_logged_and_paused(self):
        with patch.object(build, "PLATFORM_KEY", "windows"), patch.object(
            build, "PLATFORM_LABEL", "Windows"
        ), patch.object(build, "load_version_state", return_value=published_state()), patch.object(
            build, "sync_project_versions"
        ), patch.object(build, "check_npm", return_value=True), patch.object(
            build, "check_rust", return_value=True
        ), patch.object(build, "install_deps", return_value=True), patch.object(
            build, "load_product_name", return_value="Agents PM Tool"
        ), patch.object(build, "run_build", side_effect=OSError("构建命令无法启动")), patch(
            "builtins.input", side_effect=["", ""]
        ) as prompt:
            output = io.StringIO()
            with redirect_stdout(output):
                self.assertEqual(build.main(), 1)
        self.assertIn("构建命令无法启动", output.getvalue())
        self.assertEqual(prompt.call_count, 2)


if __name__ == "__main__":
    unittest.main()
