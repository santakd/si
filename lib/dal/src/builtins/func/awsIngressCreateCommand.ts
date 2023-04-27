async function create(component: Input): Promise<Output> {
  if (component.properties.resource?.payload) {
    return {
      status: "error",
      message: "Resource already exists",
      payload: component.properties.resource.payload,
    }
  }

  // Now, create the ingress
  const child = await siExec.waitUntilEnd("aws", [
    "ec2",
    "authorize-security-group-ingress",
    "--region",
    component.properties.domain.region,
    "--cli-input-json",
    component.properties.code["si:generateAwsIngressJSON"]?.code,
  ]);

  if (child.exitCode !== 0) {
    console.error(child.stderr);
    return {
      status: "error",
      message: `Unable to create Ingress, AWS CLI 2 exited with non zero code: ${child.exitCode}`,
    }
  }

  return { payload: JSON.parse(child.stdout).SecurityGroupRules, status: "ok" };
}
