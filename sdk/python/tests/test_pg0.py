"""Tests for pg0 Python client."""

import pytest
import pg0
from pg0 import Pg0, InstanceInfo, Pg0AlreadyRunningError


# Use a unique port to avoid conflicts
TEST_PORT = 15432
TEST_NAME = "pytest-test"


@pytest.fixture
def clean_instance():
    """Ensure test instance is dropped before and after test."""
    # Cleanup before - drop to remove any existing data/config
    pg0.drop(TEST_NAME)

    yield

    # Cleanup after
    pg0.drop(TEST_NAME)


class TestPg0:
    """Tests for Pg0 class."""

    def test_start_stop(self, clean_instance):
        """Test starting and stopping Pg0."""
        pg = Pg0(name=TEST_NAME, port=TEST_PORT)

        # Start
        info = pg.start()
        assert info.running is True
        assert info.port == TEST_PORT
        assert info.uri is not None
        assert f":{TEST_PORT}/" in info.uri

        # Stop
        pg.stop()
        info = pg.info()
        assert info.running is False

    def test_context_manager(self, clean_instance):
        """Test using Pg0 as context manager."""
        with Pg0(name=TEST_NAME, port=TEST_PORT) as pg:
            assert pg.running is True
            assert pg.uri is not None

        # Should be stopped after exiting context
        info = pg0.info(TEST_NAME)
        assert info.running is False

    def test_execute_sql(self, clean_instance):
        """Test executing SQL commands."""
        pg = Pg0(name=TEST_NAME, port=TEST_PORT)
        pg.start()

        try:
            # Execute a simple query
            result = pg.execute("SELECT 1 as num;")
            assert "1" in result

            # Create and query a table
            pg.execute("CREATE TABLE test_table (id serial, name text);")
            pg.execute("INSERT INTO test_table (name) VALUES ('hello');")
            result = pg.execute("SELECT name FROM test_table;")
            assert "hello" in result
        finally:
            pg.stop()

    def test_custom_credentials(self, clean_instance):
        """Test custom username, password, database."""
        pg = Pg0(
            name=TEST_NAME,
            port=TEST_PORT,
            username="testuser",
            password="testpass",
            database="testdb",
        )
        info = pg.start()

        try:
            assert "testuser" in info.uri
            assert "testpass" in info.uri
            assert "testdb" in info.uri
        finally:
            pg.stop()

    def test_custom_config(self, clean_instance):
        """Test custom Pg0 configuration."""
        pg = Pg0(
            name=TEST_NAME,
            port=TEST_PORT,
            config={"work_mem": "128MB"},
        )
        pg.start()

        try:
            result = pg.execute("SHOW work_mem;")
            assert "128MB" in result
        finally:
            pg.stop()

    def test_already_running_error(self, clean_instance):
        """Test that starting twice raises error."""
        pg = Pg0(name=TEST_NAME, port=TEST_PORT)
        pg.start()

        try:
            with pytest.raises(Pg0AlreadyRunningError):
                pg.start()
        finally:
            pg.stop()

    def test_stop_when_not_running(self, clean_instance):
        """Test that stopping when not running does not raise error."""
        pg = Pg0(name=TEST_NAME, port=TEST_PORT)
        # Should not raise - stop is idempotent
        pg.stop()

    def test_info_when_not_running(self, clean_instance):
        """Test getting info when not running."""
        pg = Pg0(name=TEST_NAME, port=TEST_PORT)
        info = pg.info()

        assert info.running is False
        assert info.uri is None


class TestConvenienceFunctions:
    """Tests for module-level convenience functions."""

    def test_start_stop_info(self, clean_instance):
        """Test start, stop, info functions."""
        info = pg0.start(name=TEST_NAME, port=TEST_PORT)
        assert info.running is True

        info = pg0.info(TEST_NAME)
        assert info.running is True
        assert info.port == TEST_PORT

        pg0.stop(TEST_NAME)
        info = pg0.info(TEST_NAME)
        assert info.running is False

    def test_list_instances(self, clean_instance):
        """Test listing instances."""
        # Start an instance
        pg0.start(name=TEST_NAME, port=TEST_PORT)

        try:
            instances = pg0.list_instances()
            names = [i.name for i in instances]
            assert TEST_NAME in names
        finally:
            pg0.stop(TEST_NAME)


class TestInstanceInfo:
    """Tests for InstanceInfo dataclass."""

    def test_from_dict(self):
        """Test creating InstanceInfo from dict."""
        data = {
            "name": "test",
            "running": True,
            "pid": 1234,
            "port": 5432,
            "uri": "postgresql://localhost:5432/test",
        }
        info = InstanceInfo.from_dict(data)

        assert info.name == "test"
        assert info.running is True
        assert info.pid == 1234
        assert info.port == 5432
        assert info.uri == "postgresql://localhost:5432/test"

    def test_from_dict_minimal(self):
        """Test creating InstanceInfo from minimal dict."""
        data = {"running": False}
        info = InstanceInfo.from_dict(data)

        assert info.name == "default"
        assert info.running is False
        assert info.pid is None
        assert info.uri is None
