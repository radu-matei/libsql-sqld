import concurrent.futures
import libsql_client
import pytest

@pytest.mark.asyncio
async def test_execute(url):
    async with libsql_client.Client(url) as client:
        result_set = await client.execute("SELECT 42")
        assert len(result_set.columns) == 1
        assert len(result_set.rows) == 1

        row = result_set.rows[0]
        assert len(row) == 1
        assert row[0] == 42

@pytest.mark.asyncio
async def test_batch(url):
    async with libsql_client.Client(url) as client:
        result_sets = await client.batch(["SELECT 42", ("VALUES (?, ?)", [1, "one"])])
        assert len(result_sets) == 2
        assert result_sets[0].rows[0][0] == 42
        assert result_sets[1].rows[0][0] == 1
        assert result_sets[1].rows[0][1] == "one"

@pytest.mark.asyncio
async def test_transaction(url):
    async with libsql_client.Client(url) as client:
        result_sets = await client.transaction(["SELECT 42", "SELECT 'two'"])
        assert len(result_sets) == 2
        assert result_sets[0].rows[0][0] == 42
        assert result_sets[1].rows[0][0] == "two"

@pytest.mark.asyncio
async def test_error(url):
    async with libsql_client.Client(url) as client:
        with pytest.raises(libsql_client.ClientResponseError) as excinfo:
            await client.execute("SELECT foo")
        assert "no such column: foo" in str(excinfo.value)

@pytest.mark.asyncio
async def test_custom_executor(url):
    with concurrent.futures.ThreadPoolExecutor(1) as executor:
        async with libsql_client.Client(url, executor=executor) as client:
            result_set = await client.execute("SELECT 42")
            assert result_set.rows[0][0] == 42
