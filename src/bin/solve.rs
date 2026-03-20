use leetcode_tui::api::{load_session, LeetCodeClient};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let session = load_session().expect("No session found at ~/.leetcode-tui/session.json");
    let client = LeetCodeClient::new(session);

    // Test problem 26 - Remove Duplicates from Sorted Array
    let slug = "remove-duplicates-from-sorted-array";
    println!("Fetching {}...", slug);
    let problem = client.get_problem(slug).await?;
    println!("Problem: {} (id={})", problem.title, problem.id);
    println!("Test cases: {:?}", problem.examples);
    println!();

    // Python solution
    let code = r#"class Solution:
    def removeDuplicates(self, nums: List[int]) -> int:
        k = 0
        for n in nums:
            if k == 0 or nums[k-1] != n:
                nums[k] = n
                k += 1
        return k
"#;

    let test_input = problem.examples.join("\n");
    println!("--- Run Tests (data_input: {:?}) ---", test_input);
    match client.run_tests(slug, code, "python3", &test_input, &problem.id.to_string()).await {
        Ok(r) => {
            println!("Status  : {}", r.status);
            println!("Passed  : {}/{}", r.passed, r.total);
            if let Some(d) = r.error_detail { println!("Detail  : {}", d); }
        }
        Err(e) => println!("Error: {e}"),
    }

    println!();
    println!("--- Submit ---");
    match client.submit(slug, code, "python3", &problem.id.to_string()).await {
        Ok(r) => {
            println!("Status  : {}", r.status);
            println!("Passed  : {}/{}", r.passed, r.total);
            if let Some(ms) = r.runtime_ms { println!("Runtime : {}ms", ms); }
            if let Some(mb) = r.memory_mb { println!("Memory  : {:.1}MB", mb); }
            if let Some(d) = r.error_detail { println!("Detail  :\n{}", d); }
        }
        Err(e) => println!("Error: {e}"),
    }

    Ok(())
}
