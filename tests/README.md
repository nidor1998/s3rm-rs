# End-to-End Tests

## Warning

These tests will create and delete AWS resources (S3 buckets and objects), which will result in costs on your AWS account.

These tests are designed to be run against a real AWS account. If any of the tests fail, they may leave resources in your AWS account, such as S3 buckets and their contents.

## Running the tests against AWS

Before running the tests, you need to set up your AWS credentials. Create a profile named `s3rm-e2e-test` with the AWS CLI:

```bash
aws configure --profile s3rm-e2e-test
```

Then run the tests with the `e2e_test` cfg flag:

```bash
# Run all E2E tests
RUSTFLAGS='--cfg e2e_test' cargo test --test 'e2e_*' -- --nocapture

# Run a specific test suite
RUSTFLAGS='--cfg e2e_test' cargo test --test e2e_deletion -- --nocapture
```

### Express One Zone tests

Express One Zone directory bucket tests use the `S3RM_E2E_AZ_ID` environment variable to select an availability zone. It defaults to `apne1-az4` if unset:

```bash
export S3RM_E2E_AZ_ID=apne1-az4
RUSTFLAGS='--cfg e2e_test' cargo test --test e2e_express_one_zone -- --nocapture
```

## Notes

These tests create and delete S3 buckets. Occasionally tests may fail due to eventual consistency in AWS (for example, a newly created bucket may not be immediately visible). In such cases, the tests will typically pass on a subsequent run.
