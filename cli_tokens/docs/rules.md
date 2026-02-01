# CLI Tools Development Rules

## Test Data and Work Files

### Work Directory Structure
```
cli_tokens/
├── cli_test/
│   └── .work/          # All test files and work data go here
│       ├── tokens/     # Downloaded/generated token files
│       ├── predictions/ # Prediction output files
│       └── temp/       # Temporary files for testing
```

### Rules for Test Data Management

1. **Work Directory Location**
   - All test files, downloaded data, and temporary work files must be stored under `cli_test/.work/`
   - This directory is excluded from git commits to keep the repository clean

2. **File Organization**
   - `cli_test/.work/tokens/` - Token data files from `top` command
   - `cli_test/.work/predictions/` - Prediction results from `predict` command
   - `cli_test/.work/temp/` - Temporary files created during testing

3. **Git Ignore Policy**
   - The entire `cli_test/.work/` directory should be added to `.gitignore`
   - Never commit test data files to the repository
   - Only commit code changes and documentation

4. **Testing Guidelines**
   - Use `cli_test/.work/` for all functional testing
   - Clean up test files periodically to avoid disk space issues
   - Use consistent naming conventions for test files

### Example Usage

```bash
# Generate test tokens
cargo run -- top --limit 3 --output cli_test/.work/tokens

# Run predictions
cargo run -- predict cli_test/.work/tokens/wrap.near.json --output cli_test/.work/predictions

# Clean up after testing
rm -rf cli_test/.work/*
```

### Benefits

- **Clean Repository**: No test data pollution in git history
- **Organized Testing**: Clear separation of test data from source code
- **Consistent Workflow**: All developers use the same directory structure
- **Easy Cleanup**: Single directory to clean for test data removal