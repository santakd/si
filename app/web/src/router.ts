import { config } from "@/config";
import { createWebHistory, createRouter, RouteRecordRaw } from "vue-router";
import { SessionService } from "@/service/session";
import Home from "@/pages/Home.vue";
import NotFoundPage from "@/pages/NotFound.vue";
import Authenticate from "@/pages/Authenticate.vue";
import Login from "@/templates/Login.vue";
import Signup from "@/templates/Signup.vue";
import Schema from "@/templates/Schema.vue";
import SchemaList from "@/organisims/Schema/SchemaList.vue";
import SchemaNew from "@/organisims/Schema/SchemaNew.vue";

const routes: RouteRecordRaw[] = [
  {
    path: "/",
    name: "home",
    component: Home,
    children: [
      {
        path: "o/:organizationId/w/:workspaceId",
        props: true,
        name: "workspace",
        redirect: { name: "application" },
      },
      {
        path: "o/:organizationId/w/:workspaceId/a",
        props: true,
        name: "application",
        component: NotFoundPage, // Application
      },
      {
        path: "o/:organizationId/w/:workspaceId/a/:applicationId",
        props: true,
        name: "applicationDetails",
        component: NotFoundPage, // ApplicationDetails
      },
      {
        path: "o/:organizationId/w/:workspaceId/s",
        props: true,
        name: "secret",
        component: NotFoundPage, // Secret
      },
      {
        path: "schema",
        props: true,
        name: "schema",
        component: Schema,
        redirect: { name: "schema-list" },
        children: [
          {
            name: "schema-list",
            path: "list",
            component: SchemaList,
            props: { modal: true },
          },
          {
            name: "schema-new",
            path: "new",
            component: SchemaNew,
          },
        ],
      },
    ],
  },
  {
    path: "/authenticate",
    name: "authenticate",
    component: Authenticate,
    // redirect: { name: "login" },
    redirect: { name: "signup" },
    children: [
      {
        path: "login",
        name: "login",
        component: Login,
      },
      {
        path: "signup",
        name: "signup",
        component: Signup,
      },
    ],
  },
];

export const routeCheck = async (to: any, _from: any, next: any) => {
  if (to.path == "/authenticate/signup" || to.path == "/authenticate/login") {
    return next();
  }

  const authenticated = await SessionService.isAuthenticated();
  if (authenticated === false || authenticated.error) {
    if (authenticated.error) {
      console.log("Error checking authentication", authenticated);
    }
    return next("/authenticate/login");
  } else {
    return next();
  }
};

const router = createRouter({
  history: createWebHistory(config.routerBase),
  routes,
});

router.beforeEach(async (to, from, next) => {
  await routeCheck(to, from, next);
});

export default router;
